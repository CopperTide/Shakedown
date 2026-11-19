"""The query root: `viewer`, `repository`, `node(s)`, `search`, `organization`, `user`, `repositoryOwner`."""
from ... import records, users
from .. import errors
from . import nodes, search
from .repository import RepositoryObj
from .viewer import Actor


class Query:
    def __init__(self, v):
        self.v = v

    @property
    def viewer(self):
        return Actor(self.v, self.v.user)

    def repository(self, owner: str, name: str, **_):
        row = records.repo_named(self.v.db, f"{owner}/{name}")
        if row is None:
            raise errors.not_found("a Repository", f"with the name '{owner}/{name}'")
        return RepositoryObj(self.v, row)

    def node(self, id: str, **_):
        return nodes.find(self.v, id)

    def nodes(self, ids: list, **_):
        return [nodes.find(self.v, i) for i in ids]

    def search(self, query: str, type: str, **page):
        return search.search(self.v, query, type, **page)

    def _login(self, login: str, kind: str | None):
        row = users.find(self.v.db, login)
        if row is None or kind and row["type"] != kind:
            what = {"Organization": "an Organization", "User": "a User"}.get(kind, "a RepositoryOwner")
            raise errors.not_found(what, f"with the login of '{login}'")
        return Actor(self.v, row)

    def organization(self, login: str, **_):
        return self._login(login, "Organization")

    def user(self, login: str, **_):
        return self._login(login, "User")

    def repository_owner(self, login: str, **_):
        return self._login(login, None)
