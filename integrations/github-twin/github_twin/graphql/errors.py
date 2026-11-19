"""GraphQL errors as GitHub (graphql-ruby) words and shapes them: gh string-matches the wording of
missing fields to tolerate them (research §2.3), and reads `type` on execution errors."""
import re
import sys
import traceback

from graphql import GraphQLError

from ..errors import GithubError

#: graphql-core's validation messages (3.3 wording) and graphql-ruby's words for them.
RUBY = [
    (re.compile(r"^Cannot query field '(\w+)' on type '(\w+)'\..*$", re.S), r"Field '\1' doesn't exist on type '\2'"),
    (re.compile(r"^Unknown argument '(\w+)' on field '\w+\.(\w+)'\..*$", re.S), r"Field '\2' doesn't accept argument '\1'"),
    (re.compile(r"^Argument '\w+\.(\w+)\((\w+):\)' of type '[^']+' is required, but it was not provided\.$"),
     r"Field '\1' is missing required arguments: \2"),
    (re.compile(r"^Unknown type '(\w+)'\..*$", re.S), r"No such type \1, so it can't be a fragment condition"),
    (re.compile(r"^Fragment cannot be spread here as objects of type '(\w+)' can never be of type '(\w+)'\.$"),
     r"Fragment on \2 can't be spread inside \1"),
]
STATUS_TYPES = {401: "UNAUTHENTICATED", 403: "FORBIDDEN", 404: "NOT_FOUND"}
INTERNAL = "Something went wrong while executing your query. Please include the request ID in any report."


class Typed(Exception):
    """An execution error with GitHub's `type` (NOT_FOUND, FORBIDDEN, UNPROCESSABLE)."""

    def __init__(self, type_: str, message: str):
        super().__init__(message)
        self.type, self.message = type_, message


def ruby_wording(message: str) -> str:
    """graphql-ruby's words for a graphql-core validation message (unchanged when it has none)."""
    for pattern, words in RUBY:
        if pattern.match(message):
            return pattern.sub(words, message)
    return message


def not_found(kind: str, what: str) -> Typed:
    return Typed("NOT_FOUND", f"Could not resolve to {kind} {what}.")


def no_node(node_id: str) -> Typed:
    return Typed("NOT_FOUND", f"Could not resolve to a node with the global id of '{node_id}'")


def forbidden() -> Typed:
    """What a read-only agent's mutation gets (GitHub's words for a token that may not write)."""
    return Typed("FORBIDDEN", "Resource not accessible by integration")


def _typed(original) -> tuple[str, str] | None:
    if isinstance(original, Typed):
        return original.type, original.message
    if isinstance(original, GithubError):
        return STATUS_TYPES.get(original.status, "UNPROCESSABLE"), original.message
    return None


def _where(e: GraphQLError) -> dict:
    out = {}
    if e.path:
        out["path"] = list(e.path)
    if e.locations:
        out["locations"] = [{"line": loc.line, "column": loc.column} for loc in e.locations]
    return out


def invalid(e: GraphQLError) -> dict:
    """A validation error: graphql-ruby's words, and `undefinedField` for a field the type lacks."""
    message = ruby_wording(e.message)
    code = {"extensions": {"code": "undefinedField"}} if "doesn't exist on type" in message else {}
    return {**_where(e), **code, "message": message}


def shaped(e: GraphQLError) -> dict:
    """An execution error as GitHub answers it: GitHub's `type` for a refusal; a bug is logged and
    answered in GitHub's words for an internal error."""
    typed = _typed(e.original_error)
    if typed is not None:
        return {"type": typed[0], **_where(e), "message": typed[1]}
    if e.original_error is not None:
        traceback.print_exception(e.original_error, file=sys.stderr)
        return {**_where(e), "message": INTERNAL}
    return {**_where(e), "message": e.message}
