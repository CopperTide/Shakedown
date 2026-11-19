"""``gpupod``: the pod's operations from the command line.

    uv run --project deploy/runpod gpupod quote
    uv run --project deploy/runpod gpupod start --confirm-rate 2.69 [--hours 8]
    uv run --project deploy/runpod gpupod smoke [--through-arenad http://127.0.0.1:4510]
    uv run --project deploy/runpod gpupod status | stop | delete [--force] | reconcile
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path
from typing import Callable, Optional

import httpx

from .ledger import Ledger
from .pod import Ops, delete, quote, read_env, reconcile, start, status, stop
from .runpodctl import RealRunner, RunpodError
from .smoke import smoke
from .spec import POD_TOML, load_spec
from .watchdog import watch

LEDGER = POD_TOML.parent / "ledger.jsonl"
ENV_FILE = Path.home() / ".config" / "shakedown" / "runpod.env"
STATE_DIR = Path.home() / ".local" / "state" / "shakedown"


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="gpupod", description="Shakedown's RunPod pod for its custom model")
    p.add_argument("--ledger", default=str(LEDGER))
    p.add_argument("--env-file", default=str(ENV_FILE))
    p.add_argument("--state-dir", default=str(STATE_DIR))
    p.add_argument("--arenad", default=os.environ.get("ARENA_URL") or "http://127.0.0.1:4510")
    sub = p.add_subparsers(dest="cmd", required=True)
    q = sub.add_parser("quote", help="the GPU's hourly rate now, and the spend so far")
    q.add_argument("--gpu")
    q.add_argument("--hours", type=float)
    s = sub.add_parser("start", help="start the pod at the confirmed rate, within the budget")
    s.add_argument("--confirm-rate", type=float, required=True)
    s.add_argument("--gpu")
    s.add_argument("--hours", type=float, default=8.0)
    s.add_argument("--low-memory", action="store_true")
    for name in ("status", "stop", "reconcile"):
        sub.add_parser(name)
    sub.add_parser("delete").add_argument("--force", action="store_true")
    sub.add_parser("smoke").add_argument("--through-arenad")
    sub.add_parser("watch", help="the watchdog (start runs it)").add_argument("pod_id")
    return p


def real_ops(args: argparse.Namespace) -> Ops:
    return Ops(runner=RealRunner(), ledger=Ledger(Path(args.ledger)), spec=load_spec(),
               env_path=Path(args.env_file), state_dir=Path(args.state_dir),
               http=httpx.Client(timeout=30), arenad_url=args.arenad)


def _smoke(ops: Ops, through: Optional[str]) -> int:
    s = ops.ledger.open_session()
    env = read_env(ops)
    name = ops.spec.arenad_provider
    url = env.get("ARENA_LOCAL_MODELS", "").removeprefix(f"{name}=")
    key = env.get(f"ARENA_LOCAL_KEY_{name.upper()}")
    if not (s and url and key):
        ops.out(f"No started pod in {ops.env_path}: run gpupod start first.")
        return 2
    report = smoke(ops, url.removesuffix("/v1"), key, ops.spec.gpu(s["gpu_id"]).model, through)
    for line in report.lines():
        ops.out(line)
    if report.decode_tok_s is not None:
        ops.ledger.append({"kind": "measure", "pod_id": s["pod_id"], "decode_tok_s": report.decode_tok_s})
        ops.out(f"decode: {report.decode_tok_s} tok/s")
    return 0 if report.ok else 1


def _watch(ops: Ops, pod_id: str) -> int:
    try:
        watch(ops, pod_id)
    finally:
        (ops.state_dir / "runpod-watchdog.pid").unlink(missing_ok=True)
    return 0


def _run(ops: Ops, args: argparse.Namespace) -> int:
    commands: dict[str, Callable[[], int]] = {
        "quote": lambda: quote(ops, args.gpu, args.hours),
        "start": lambda: start(ops, args.confirm_rate, args.gpu, args.hours, args.low_memory),
        "status": lambda: status(ops),
        "stop": lambda: stop(ops),
        "delete": lambda: delete(ops, args.force),
        "reconcile": lambda: reconcile(ops),
        "smoke": lambda: _smoke(ops, args.through_arenad),
        "watch": lambda: _watch(ops, args.pod_id),
    }
    return commands[args.cmd]()


def main(argv: Optional[list[str]] = None,
         make_ops: Callable[[argparse.Namespace], Ops] = real_ops) -> int:
    args = parser().parse_args(argv)
    ops = make_ops(args)
    try:
        return _run(ops, args)
    except RunpodError as e:
        ops.out(f"runpodctl failed: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
