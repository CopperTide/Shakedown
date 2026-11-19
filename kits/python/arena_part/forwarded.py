"""The public base URL of a request that came through arenad's proxy, for the absolute URLs a
part mints (docs/protocol.md, "Serving HTTP")."""
from typing import Mapping


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
