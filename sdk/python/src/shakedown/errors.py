"""Errors the SDK raises. Every error from shaked carries its HTTP status and shaked's message."""

from __future__ import annotations


class ShakedownError(Exception):
    """Base class of every error the SDK raises."""


#: The old name (the SDK was ``arena`` until 2026-11-19).
ArenaError = ShakedownError


class ConnectionFailed(ShakedownError):
    """shaked could not be reached (not running, wrong URL, or it went away)."""


class ApiError(ShakedownError):
    """shaked answered with an error status. ``status`` is the HTTP status, ``message`` shaked's
    ``{error}`` text."""

    def __init__(self, status: int, message: str, method: str = "", path: str = ""):
        self.status = status
        self.message = message
        self.method = method
        self.path = path
        where = f" ({method} {path})" if method else ""
        super().__init__(f"{status}: {message}{where}")


class BadRequest(ApiError):
    """400: the request was refused as given (unknown task, bad team, bad scenario, ...)."""


class NotFound(ApiError):
    """404: no such run, branch, agent or route."""


class Conflict(ApiError):
    """409: not now (a branch is running, a fork can't restore a part, a cleanup runs, ...)."""


class DiskFull(ApiError):
    """507: the saves reached shaked's disk limit; delete old runs."""


class NotSupported(ShakedownError):
    """The connected shaked doesn't have this feature (for example pause and resume)."""


def from_status(status: int, message: str, method: str = "", path: str = "") -> ApiError:
    cls = {400: BadRequest, 404: NotFound, 409: Conflict, 507: DiskFull}.get(status, ApiError)
    return cls(status, message, method, path)
