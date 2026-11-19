"""Webhook deliveries: queued in the request's transaction, sent by a worker after it commits, only
to this machine (a world is sealed); signed with the hook's secret."""
import hashlib
import hmac
import json
import sqlite3
import sys
import threading
import urllib.error
import urllib.request
import uuid
from urllib.parse import urlsplit

LOOPBACK = {"127.0.0.1", "localhost", "::1"}
OUTSIDE = "outside the world"
USER_AGENT = "GitHub-Hookshot/shakedown"


def hooks_for(db, repo_id: int, event: str) -> list:
    rows = db.execute("SELECT * FROM hooks WHERE repo_id = ? AND active = 1 ORDER BY id", (repo_id,)).fetchall()
    return [h for h in rows if event in json.loads(h["events"]) or "*" in json.loads(h["events"])]


def queue_one(ctx, hook, event: str, payload: dict) -> None:
    ctx.store.conn.execute("INSERT INTO deliveries(hook_id, event, guid, payload, created_ms) VALUES (?, ?, ?, ?, ?)",
                           (hook["id"], event, str(uuid.uuid4()), json.dumps(payload), ctx.now_ms()))


def queue(ctx, repo_id: int, event: str, payload: dict) -> None:
    for hook in hooks_for(ctx.store.conn, repo_id, event):
        queue_one(ctx, hook, event, payload)


def _headers(hook, d, body: bytes) -> dict:
    cfg = json.loads(hook["config"])
    h = {"Content-Type": "application/json", "User-Agent": USER_AGENT, "X-GitHub-Event": d["event"],
         "X-GitHub-Delivery": d["guid"], "X-GitHub-Hook-ID": str(hook["id"]),
         "X-GitHub-Hook-Installation-Target-Type": "repository",
         "X-GitHub-Hook-Installation-Target-ID": str(hook["repo_id"])}
    if cfg.get("secret"):
        h["X-Hub-Signature-256"] = "sha256=" + hmac.new(cfg["secret"].encode(), body, hashlib.sha256).hexdigest()
    return h


def send(hook, d) -> tuple[int | None, str | None]:
    """POST one delivery; (status code, error). Never leaves the machine."""
    url = json.loads(hook["config"]).get("url", "")
    if urlsplit(url).hostname not in LOOPBACK:
        return None, OUTSIDE
    body = d["payload"].encode()
    req = urllib.request.Request(url, data=body, method="POST", headers=_headers(hook, d, body))
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, None
    except urllib.error.HTTPError as e:
        return e.code, f"HTTP {e.code}"
    except (OSError, ValueError) as e:
        return None, str(e)


class Deliverer:
    """One worker per instance: takes committed pending deliveries under the instance's lock, sends
    them without it, and records what happened."""

    def __init__(self, ctx, poll_s: float = 0.1):
        self.ctx, self.poll_s = ctx, poll_s
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="github-hooks", daemon=True)

    def start(self) -> "Deliverer":
        self._thread.start()
        return self

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=15)

    def _pending(self) -> list:
        with self.ctx.lock:
            db = self.ctx.store.conn
            return [(db.execute("SELECT * FROM hooks WHERE id = ?", (d["hook_id"],)).fetchone(), d) for d in
                    db.execute("SELECT * FROM deliveries WHERE state = 'pending' ORDER BY id").fetchall()]

    def _record(self, hook, d, code, error) -> None:
        with self.ctx.lock, self.ctx.store.tx() as db:
            db.execute("UPDATE deliveries SET state = ?, status_code = ?, error = ? WHERE id = ?",
                       ("failed" if error else "delivered", code, error, d["id"]))
            if hook is not None:
                db.execute("UPDATE hooks SET last_code = ?, last_status = ?, last_message = ? WHERE id = ?",
                           (code, "failed" if error else "active", error or "OK", hook["id"]))

    def _round(self) -> None:
        for hook, d in self._pending():
            code, error = send(hook, d) if hook is not None else (None, "hook deleted")
            self._record(hook, d, code, error)

    def _run(self) -> None:
        while not self._stop.wait(self.poll_s):
            try:
                self._round()
            except sqlite3.Error as e:   # one bad round is reported, not the end of deliveries
                print(f"github-twin: webhook delivery failed: {e}", file=sys.stderr)
