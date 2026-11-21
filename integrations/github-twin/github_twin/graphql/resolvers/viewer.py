"""Users, bots and organizations: the viewer, authors, assignees, reviewers and repository owners."""
from ... import users
from ..paging import empty
from .common import Obj, when

KINDS = ("User", "Bot", "Organization")


class Actor(Obj):
    def __init__(self, v, row):
        super().__init__(v)
        self.row = row
        self.typename = row["type"] if row["type"] in KINDS else "User"
        self.key = self.database_id = row["id"]
        self.login = row["login"]

    @property
    def name(self) -> str:
        return self.login

    @property
    def url(self) -> str:
        return self.v.urls.html(f"/{self.login}")

    @property
    def resource_path(self) -> str:
        return f"/{self.login}"

    def avatar_url(self, size=None, **_) -> str:
        return self.v.urls.html(f"/avatars/u/{self.key}")

    @property
    def created_at(self) -> str | None:
        return when(self.row["created_ms"])

    @property
    def updated_at(self) -> str | None:
        return when(self.row["created_ms"])

    def teams(self, **_) -> dict:
        return empty()


def actor(v, login: str | None) -> Actor | None:
    """The user named `login` (made on first sight, as REST does); None for no login."""
    if not login:
        return None
    return Actor(v, users.find(v.db, login) or users.ensure(v.db, login, v.now()))


def by_kind(v, kind: str | None = None) -> list:
    rows = v.db.execute("SELECT * FROM users ORDER BY login").fetchall()
    return [Actor(v, r) for r in rows if kind is None or r["type"] == kind]
