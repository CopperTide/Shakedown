"""The token a request carries: `token X`, `Bearer X`, or a Basic password (git's credential helper)."""
import base64
import hmac


def token_of(headers) -> str | None:
    value = headers.get("authorization", "")
    scheme, _, rest = value.partition(" ")
    if scheme.lower() in ("token", "bearer"):
        return rest.strip() or None
    if scheme.lower() == "basic":
        try:
            return base64.b64decode(rest.strip()).decode().partition(":")[2] or None
        except (ValueError, UnicodeDecodeError):
            return None
    return None


def check(headers, token: str) -> str:
    """`ok`, `missing` (no credentials) or `bad` (credentials that are not the twin's token)."""
    given = token_of(headers)
    if given is None:
        return "missing"
    return "ok" if hmac.compare_digest(given.encode(), token.encode()) else "bad"
