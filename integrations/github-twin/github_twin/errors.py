"""A refusal with GitHub's status and words, raised by the rules and answered by REST or an act."""


class GithubError(Exception):
    def __init__(self, status: int, message: str, errors: list | None = None, headers: dict | None = None):
        super().__init__(message)
        self.status, self.message, self.errors, self.headers = status, message, errors, headers


def not_found() -> GithubError:
    return GithubError(404, "Not Found")


def invalid(message: str, errors: list | None = None) -> GithubError:
    return GithubError(422, message, errors)


def read_only() -> GithubError:
    """What a read-only agent's write gets (GitHub's words for a token that may not write)."""
    return GithubError(403, "Resource not accessible by integration")
