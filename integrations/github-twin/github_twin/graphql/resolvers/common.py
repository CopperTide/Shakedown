"""What the resolved objects share: the request context, node ids and GitHub's time format.

A field resolves to the object's attribute or method of its name in snake case (see the executor);
a field an object lacks gets its type's empty value."""
from ...times import iso
from ...users import node_id


class Obj:
    """A resolved GraphQL object; `typename` is its concrete type for unions and interfaces."""
    typename = ""

    def __init__(self, v):
        self.v = v

    @property
    def id(self) -> str:
        return node_id(self.typename, self.key)

    key: object = None


def when(ms: int | None) -> str | None:
    return iso(ms) if ms else None


def upper(value: str | None) -> str | None:
    return value.upper() if value else None


def repository_of(v, repo):
    """The `Repository` object of a repos row (imported here: the repository module imports the rest)."""
    from .repository import RepositoryObj
    return RepositoryObj(v, repo)
