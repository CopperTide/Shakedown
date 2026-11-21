"""Slack's ids (`U…`, `C…`, `T…`: a letter and ten of A-Z0-9, made from a name so a world is the same
every run) and timestamps (`1773122400.000000`: world µs, unique in the workspace)."""
import base64
import hashlib


def make(prefix: str, name: str) -> str:
    digest = hashlib.sha256(f"slack:{prefix}:{name}".encode()).digest()
    return prefix + base64.b32encode(digest).decode()[:10].replace("=", "A")


def ts(us: int) -> str:
    return f"{us // 1_000_000}.{us % 1_000_000:06d}"


def parse_ts(text) -> int | None:
    """A `ts` as µs; None when it is not one."""
    secs, _, frac = str(text or "").partition(".")
    if not secs.isdigit() or (frac and not frac.isdigit()):
        return None
    return int(secs) * 1_000_000 + int((frac + "000000")[:6])


def next_us(db, now_ms: int) -> int:
    """World now in µs, after every message already stamped."""
    last = db.execute("SELECT COALESCE(MAX(ts_us), 0) FROM messages").fetchone()[0]
    return max(now_ms * 1000, last + 1)
