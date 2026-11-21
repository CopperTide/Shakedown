"""Refusals in PagerDuty's words: REST `{"error": {"message", "code", "errors"?}}`, Events API
`{"status", "message", "errors"}`."""


class PagerDutyError(Exception):
    def __init__(self, status: int, message: str, code: int, errors: list | None = None):
        super().__init__(message)
        self.status, self.message, self.code, self.errors = status, message, code, errors

    def body(self) -> dict:
        err = {"message": self.message, "code": self.code}
        if self.errors:
            err["errors"] = self.errors
        return {"error": err}


def not_found() -> PagerDutyError:
    return PagerDutyError(404, "Not Found", 2100)


def invalid(*errors: str) -> PagerDutyError:
    return PagerDutyError(400, "Invalid Input Provided", 2001, list(errors))


NO_FROM = PagerDutyError(400, "You must specify a user's email address in the \"From\" header", 2001)
UNAUTHORIZED = PagerDutyError(401, "Authentication failed", 2006)
ACCESS_DENIED = PagerDutyError(403, "Access Denied", 2010)


class EventError(Exception):
    def __init__(self, *errors: str, status: int = 400, message: str = "Event object is invalid"):
        super().__init__(message)
        self.status, self.message, self.errors = status, message, list(errors)

    def body(self) -> dict:
        return {"status": "invalid event" if self.status == 400 else "forbidden", "message": self.message,
                "errors": self.errors}
