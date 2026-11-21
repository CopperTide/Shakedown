"""The pod's life: quote, start within the budget, stop, delete, reconcile with billing.

``start`` prints the hourly rate first, then refuses when the confirmed rate is not the rate now,
when the GPU wanted has no stock (naming the next one in order; it never falls back by itself),
or when the ledger's spend plus the rate × the planned hours would pass the budget.
"""

from __future__ import annotations

import json
import os
import secrets
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from pathlib import Path
from typing import Callable, Optional

import httpx

from .ledger import AT, Ledger
from .prices import Quote, current_rate, quote_line, utc_now
from .runpodctl import RunpodError, Runner, call
from .spec import Gpu, PodSpec, proxy_base

HEALTH_WAIT = timedelta(minutes=20)
HEALTH_POLL_S = 15.0
RATE_SLACK = 0.005


def spawn_detached(argv: list[str], log: Path) -> int:
    """Starts ``argv`` in a session of its own (it outlives this command), logging to ``log``."""
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("a") as f:
        p = subprocess.Popen(argv, stdout=f, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                             start_new_session=True)
    return p.pid


@dataclass
class Ops:
    """What the operations act on; the CLI wires the real ones, tests wire fakes."""

    runner: Runner
    ledger: Ledger
    spec: PodSpec
    env_path: Path
    state_dir: Path
    http: httpx.Client
    out: Callable[[str], None] = print
    now: Callable[[], datetime] = utc_now
    sleep: Callable[[float], None] = time.sleep
    spawn: Callable[[list[str], Path], int] = field(default=spawn_detached)
    shaked_url: str = "http://127.0.0.1:4510"

    def base(self, pod_id: str) -> str:
        return proxy_base(pod_id, self.spec.port)


# ---------------------------------------------------------------------------- quote


def quote(ops: Ops, gpu_id: Optional[str] = None, hours: Optional[float] = None) -> int:
    q = _quote(ops, ops.spec.gpu(gpu_id), hours)
    _warn_balance(ops)
    return 0 if q.usd_h is not None else 2


def _quote(ops: Ops, gpu: Gpu, hours: Optional[float]) -> Quote:
    """Reads the rate now, records it and prints it: the first thing every start says."""
    q = current_rate(ops.runner, gpu.id, ops.spec.cloud, now=ops.now)
    ops.ledger.append(q.entry())
    h = hours if hours is not None else ops.spec.max_session_hours
    ops.out(quote_line(q, ops.ledger.spent(), ops.spec.budget_usd, h))
    return q


def _warn_balance(ops: Ops) -> None:
    """Warns when the account's balance is above what is left of the budget (the one stop that
    works if this machine dies with a pod running is a balance that cannot pay for more)."""
    try:
        balance = float(call(ops.runner, "user")["clientBalance"])
    except (RunpodError, KeyError, TypeError, ValueError) as e:
        ops.out(f"Could not read the RunPod balance: {e}")
        return
    left = ops.spec.budget_usd - ops.ledger.spent()
    if balance > left + RATE_SLACK:
        ops.out(f"Warning: the RunPod balance is ${balance:.2f}, more than the ${left:.2f} left of "
                f"the budget. Keep it at or below that, with auto-reload off.")


# ---------------------------------------------------------------------------- start


def start(ops: Ops, confirm_rate: float, gpu_id: Optional[str] = None, hours: float = 8.0,
          low_memory: bool = False) -> int:
    """Quotes, checks, then starts the pod (re-using a stopped one), writes ``runpod.env``,
    starts the watchdog and waits for vLLM's health. 0 ready, 1 unhealthy, 2 refused."""
    gpu = ops.spec.gpu(gpu_id)
    q = _quote(ops, gpu, hours)
    why = _refusal(ops, gpu, q, confirm_rate, hours)
    if why:
        ops.out(why)
        return 2
    _warn_balance(ops)
    up = _bring_up(ops, gpu, low_memory)
    if isinstance(up, str):
        ops.out(up)
        return 2
    pod_id, key, reused = up
    ops.ledger.append({"kind": "start", "pod_id": pod_id, "gpu_id": gpu.id, "cloud": q.cloud,
                       "usd_h": q.usd_h, "hours_planned": hours, "reused": reused})
    write_env(ops, pod_id, key)
    _start_watchdog(ops, pod_id)
    return _until_ready(ops, pod_id)


def _refusal(ops: Ops, gpu: Gpu, q: Quote, confirm: float, hours: float) -> Optional[str]:
    if not q.available:
        return _no_stock(ops, gpu)
    if abs(q.usd_h - confirm) > RATE_SLACK:
        return (f"Refused: the rate is now ${q.usd_h:.2f}/h, not the ${confirm:.2f}/h confirmed. "
                f"Tell the owner the new rate, then start with --confirm-rate {q.usd_h:.2f}.")
    spent = ops.ledger.spent()
    projected = round(spent + q.usd_h * hours, 2)
    if projected > ops.spec.budget_usd:
        return (f"Refused: starting would bring the spend to ${projected:.2f}, over the "
                f"${ops.spec.budget_usd:.2f} budget (spent ${spent:.2f}, plus ${q.usd_h:.2f}/h × "
                f"{hours:g} h). Plan fewer hours or stop here.")
    return None


def _no_stock(ops: Ops, gpu: Gpu) -> str:
    """Names the next GPU in order that has stock, with its rate; never starts it."""
    cloud = ops.spec.cloud
    for nxt in ops.spec.after(gpu):
        q = current_rate(ops.runner, nxt.id, cloud, now=ops.now)
        if q.available:
            return (f"Refused: {gpu.id} has no stock on {cloud.lower()} cloud now. The next in order "
                    f"is {nxt.id} at ${q.usd_h:.2f}/h ({nxt.model}); tell the owner, then: "
                    f"gpupod start --gpu '{nxt.id}' --confirm-rate {q.usd_h:.2f}")
    return f"Refused: {gpu.id} has no stock, and no GPU after it in pod.toml has any now."


def _bring_up(ops: Ops, gpu: Gpu, low_memory: bool) -> tuple[str, str, bool] | str:
    """Starts the session's stopped pod again, or creates one: ``(pod_id, key, reused)``, or
    why neither can be done."""
    s = ops.ledger.open_session()
    if s and not s["stopped"]:
        return f"Refused: pod {s['pod_id']} is already running; stop it first (gpupod stop)."
    if s and s["gpu_id"] != gpu.id:
        return f"Refused: the stopped pod {s['pod_id']} has {s['gpu_id']}; delete it first (gpupod delete)."
    if s:
        key = read_env(ops).get(f"SHAKEDOWN_LOCAL_KEY_{ops.spec.shaked_provider.upper()}")
        if not key:
            return f"Refused: {ops.env_path} has no key for pod {s['pod_id']}; delete the pod and start again."
        call(ops.runner, "pod", "start", s["pod_id"])
        return s["pod_id"], key, True
    key = secrets.token_urlsafe(32)
    env = {**ops.spec.env, "VLLM_API_KEY": key}
    pod = call(ops.runner, *create_argv(ops.spec, gpu, env, low_memory))
    return pod["id"], key, False


def create_argv(spec: PodSpec, gpu: Gpu, env: dict, low_memory: bool) -> list[str]:
    """``pod create`` with the image, disks, port, env (with the key) and vLLM's arguments."""
    args = [a.format(model=gpu.model, revision=gpu.revision) for a in spec.serve_args]
    if low_memory:
        args += spec.low_memory_args
    return ["pod", "create", "--name", spec.name, "--image", spec.image, "--gpu-id", gpu.id,
            "--cloud-type", spec.cloud, "--min-cuda-version", spec.min_cuda,
            "--container-disk-in-gb", str(spec.container_disk_gb), "--volume-in-gb", str(spec.volume_gb),
            "--volume-mount-path", spec.volume_mount, "--ports", f"{spec.port}/http",
            "--env", json.dumps(env), "--docker-args", " ".join(args)]


# ---------------------------------------------------------------------------- runpod.env


def write_env(ops: Ops, pod_id: str, key: str) -> None:
    """``runpod.env`` (mode 0600): what shaked needs to reach the pod as ``local:<provider>``."""
    name, (p_in, p_out) = ops.spec.shaked_provider, ops.spec.hosted_price_per_mtok
    lines = [f"SHAKEDOWN_LOCAL_MODELS={name}={ops.base(pod_id)}/v1",
             f"SHAKEDOWN_LOCAL_KEY_{name.upper()}={key}",
             f"SHAKEDOWN_LOCAL_PRICE_{name.upper()}={p_in:.2f},{p_out:.2f}"]
    ops.env_path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(ops.env_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write("\n".join(lines) + "\n")
    os.chmod(ops.env_path, 0o600)


def read_env(ops: Ops) -> dict[str, str]:
    if not ops.env_path.exists():
        return {}
    pairs = (x.split("=", 1) for x in ops.env_path.read_text().splitlines() if "=" in x)
    return {k: v for k, v in pairs}


# ---------------------------------------------------------------------------- watch and wait


def _start_watchdog(ops: Ops, pod_id: str) -> None:
    argv = [sys.executable, "-m", "gpupod.cli", "--ledger", str(ops.ledger.path),
            "--state-dir", str(ops.state_dir), "--shaked", ops.shaked_url, "watch", pod_id]
    pid = ops.spawn(argv, ops.state_dir / "runpod-watchdog.log")
    ops.state_dir.mkdir(parents=True, exist_ok=True)
    (ops.state_dir / "runpod-watchdog.pid").write_text(f"{pid}\n")
    ops.out(f"Watchdog started (pid {pid}): it stops the pod after {ops.spec.idle_minutes} idle minutes.")


def _healthy(ops: Ops, pod_id: str) -> bool:
    try:
        return ops.http.get(f"{ops.base(pod_id)}/health", timeout=30).status_code == 200
    except httpx.HTTPError:
        return False


def _until_ready(ops: Ops, pod_id: str) -> int:
    deadline = ops.now() + HEALTH_WAIT
    while not _healthy(ops, pod_id):
        if ops.now() >= deadline:
            call(ops.runner, "pod", "stop", pod_id)
            ops.ledger.append({"kind": "stop", "pod_id": pod_id, "by": "unhealthy"})
            ops.out(f"The pod was not healthy after 20 min, so it was stopped. Read why: "
                    f"runpodctl pod logs {pod_id}")
            return 1
        ops.sleep(HEALTH_POLL_S)
    ops.ledger.append({"kind": "ready", "pod_id": pod_id, "url": f"{ops.base(pod_id)}/v1"})
    ops.out(f"Ready: {ops.base(pod_id)}/v1 (public URL; /v1 needs the key in {ops.env_path}; "
            f"restart shaked with that file)")
    return 0


# ---------------------------------------------------------------------------- stop, delete, status


def stop(ops: Ops, by: str = "owner") -> int:
    s = ops.ledger.open_session()
    if not s or s["stopped"]:
        ops.out("No pod is running.")
        return 0
    call(ops.runner, "pod", "stop", s["pod_id"])
    ops.ledger.append({"kind": "stop", "pod_id": s["pod_id"], "by": by})
    ops.out(f"Stopped pod {s['pod_id']}; its volume keeps the weights. Spent ${ops.ledger.spent():.2f}.")
    return 0


def shaked_running(http: httpx.Client, url: str) -> Optional[bool]:
    """Whether any run in shaked is running (None: shaked did not answer)."""
    try:
        runs = http.get(f"{url}/api/runs", timeout=10).raise_for_status().json()
    except (httpx.HTTPError, ValueError):
        return None
    branches = [b for r in runs for b in [r, *(r.get("branches") or [])]]
    return any(b.get("status") == "running" for b in branches)


def delete(ops: Ops, force: bool = False) -> int:
    s = ops.ledger.open_session()
    if not s:
        ops.out("No pod to delete.")
        return 0
    if shaked_running(ops.http, ops.shaked_url) and not force:
        ops.out("Refused: a run in shaked is running. Wait for it, or delete with --force.")
        return 2
    call(ops.runner, "pod", "delete", s["pod_id"])
    ops.ledger.append({"kind": "delete", "pod_id": s["pod_id"]})
    ops.out(f"Deleted pod {s['pod_id']} and its volume.")
    return 0


def status(ops: Ops) -> int:
    s = ops.ledger.open_session()
    spent = ops.ledger.spent()
    ops.out(f"Spent ${spent:.2f} of ${ops.spec.budget_usd:.2f}.")
    if not s:
        ops.out("No pod.")
        return 0
    pod = call(ops.runner, "pod", "get", s["pod_id"])
    state = pod.get("desiredStatus") or pod.get("status")
    ops.out(f"Pod {s['pod_id']} ({s['gpu_id']}, ${s['usd_h']:.2f}/h): {state}, "
            f"runtime {pod.get('runtimeStatus', 'unknown')}; {ops.base(s['pod_id'])}/v1")
    return 0


# ---------------------------------------------------------------------------- reconcile


def _billed(records: object) -> float:
    rows = records.get("records", []) if isinstance(records, dict) else records
    return sum(float(r.get("totalAmount", r.get("amount", 0)) or 0) for r in rows or [])


def reconcile(ops: Ops) -> int:
    """Records each pod's billed total so far (hourly buckets since its first start)."""
    starts = [e for e in ops.ledger.entries() if e["kind"] == "start"]
    firsts = {e["pod_id"]: e["at"] for e in reversed(starts)}
    for pod_id, at in firsts.items():
        records = call(ops.runner, "billing", "pods", "--pod-id", pod_id, "--bucket-size", "hour",
                       "--start-time", at)
        usd = round(_billed(records), 4)
        ops.ledger.append({"kind": "reconcile", "pod_id": pod_id, "usd_actual": usd})
        ops.out(f"Pod {pod_id}: billed ${usd:.2f} since {at}.")
    ops.out(f"Spent ${ops.ledger.spent():.2f} of ${ops.spec.budget_usd:.2f}.")
    return 0


__all__ = ["AT", "Ops", "create_argv", "delete", "quote", "reconcile", "start", "status", "stop"]
