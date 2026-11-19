class JqlError(Exception):
    """A query Jira refuses, with its message (`Error in the JQL Query: …` for syntax)."""

    def __init__(self, message: str):
        super().__init__(message)
        self.message = message


def syntax(detail: str) -> JqlError:
    return JqlError(f"Error in the JQL Query: {detail}")
