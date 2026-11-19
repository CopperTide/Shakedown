"""A refusal as Slack answers it: HTTP 200 with `{"ok": false, "error": "<code>"}` (and its extras)."""


class SlackError(Exception):
    def __init__(self, code: str, **extra):
        super().__init__(code)
        self.code, self.extra = code, extra

    def body(self) -> dict:
        return {"ok": False, "error": self.code, **self.extra}
