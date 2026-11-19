"""The headers every REST answer carries, as GHES sends them (research §3.3)."""
import hashlib

API_VERSION = "2022-11-28"
SCOPES = "repo, read:org, workflow"
RATE_LIMIT = 5000


def api_headers(installed_version: str = "3.20.0", now_ms: int = 0) -> dict:
    return {
        "X-GitHub-Media-Type": "github.v3; format=json",
        "X-GitHub-Api-Version-Selected": API_VERSION,
        "X-OAuth-Scopes": SCOPES,
        "X-Accepted-OAuth-Scopes": "",
        "X-GitHub-Enterprise-Version": installed_version,
        "X-RateLimit-Limit": str(RATE_LIMIT),
        "X-RateLimit-Remaining": str(RATE_LIMIT - 1),
        "X-RateLimit-Used": "1",
        "X-RateLimit-Reset": str(now_ms // 1000 + 3600),
        "X-RateLimit-Resource": "core",
    }


def etag(body: bytes) -> str:
    return f'W/"{hashlib.sha256(body).hexdigest()[:32]}"'
