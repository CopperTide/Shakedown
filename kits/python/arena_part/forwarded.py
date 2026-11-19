"""What arenad's proxy tells a part about a request: the public base URL, for the absolute URLs a
part mints, the agent that sent it, and whether that agent may only read (docs/protocol.md,
"Serving HTTP")."""
from typing import Mapping

AGENT_HEADER = "x-arena-agent"
READONLY_HEADER = "x-arena-readonly"


def _header(headers: Mapping[str, str], name: str):
    return next((v for k, v in headers.items() if k.lower() == name), None)


def public_base(headers: Mapping[str, str], fallback: str) -> str:
    """`<X-Forwarded-Proto>://<X-Forwarded-Host>` (header names in any case); `fallback` (the
    part's own address) when either header is missing."""
    proto = _header(headers, "x-forwarded-proto")
    host = _header(headers, "x-forwarded-host")
    if not proto or not host:
        return fallback
    return f"{proto}://{host}"


def agent_of(headers: Mapping[str, str]) -> str | None:
    """The id of the agent that sent the request (`X-Arena-Agent`, set by the proxy, which drops a
    client's own copy); None when absent or empty, as for a request that did not come through it."""
    return _header(headers, AGENT_HEADER) or None


def readonly_of(headers: Mapping[str, str]) -> bool:
    """True when the proxy says the agent may only read (`X-Arena-Readonly: 1`); it sends that only
    to parts whose routes say `honours_readonly`, which must then refuse the agent's writes."""
    return (_header(headers, READONLY_HEADER) or "").strip().lower() in ("1", "true")
