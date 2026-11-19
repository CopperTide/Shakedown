"""A refusal with Jira's status and words (`{"errorMessages": [...], "errors": {...}}`), raised by the
rules and answered by REST or an act."""


class JiraError(Exception):
    def __init__(self, status: int, messages: list | None = None, errors: dict | None = None):
        self.status, self.messages, self.errors = status, list(messages or []), dict(errors or {})
        super().__init__("; ".join([*self.messages, *self.errors.values()]))

    def body(self) -> dict:
        return {"errorMessages": self.messages, "errors": self.errors}


def no_issue() -> JiraError:
    return JiraError(404, ["Issue does not exist or you do not have permission to see it."])


def invalid(field: str, message: str) -> JiraError:
    return JiraError(400, errors={field: message})


def bad(message: str) -> JiraError:
    return JiraError(400, [message])


def not_found(message: str) -> JiraError:
    return JiraError(404, [message])
