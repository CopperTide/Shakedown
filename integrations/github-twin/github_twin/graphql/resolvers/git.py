"""Git objects: commits (with their status check rollup) and refs."""
from ... import users
from ...repos import full_name
from ..paging import connection
from . import rollup
from .common import Obj, repository_of, when
from .viewer import Actor

HEADS = "refs/heads/"


def _person(v, p) -> dict:
    """A `GitActor`."""
    row = users.find(v.db, p.name)
    return {"name": p.name, "email": p.email, "date": when(p.at_ms), "user": Actor(v, row) if row else None,
            "avatarUrl": v.urls.html("/avatars/u/0")}


class CommitObj(Obj):
    typename = "Commit"

    def __init__(self, v, repo, sha: str):
        super().__init__(v)
        self.repo, self.oid, self.key = repo, sha, f"{repo['id']}:{sha}"
        self.abbreviated_oid = sha[:7]

    @property
    def _info(self):
        return self.v.repos.commit_info(full_name(self.repo), self.oid)

    @property
    def message(self) -> str:
        return self._info.message

    @property
    def message_headline(self) -> str:
        return self.message.split("\n", 1)[0]

    @property
    def message_body(self) -> str:
        return self.message.partition("\n")[2].strip("\n")

    @property
    def author(self) -> dict:
        return _person(self.v, self._info.author)

    @property
    def committer(self) -> dict:
        return _person(self.v, self._info.committer)

    @property
    def authored_date(self) -> str:
        return when(self._info.author.at_ms)

    @property
    def committed_date(self) -> str:
        return when(self._info.committer.at_ms)

    def authors(self, **page) -> dict:
        return connection([self.author], **page)

    def parents(self, **page) -> dict:
        return connection(self._info.parents, lambda s: CommitObj(self.v, self.repo, s), **page)

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(full_name(self.repo), f"/commit/{self.oid}")

    commit_url = url

    @property
    def status_check_rollup(self):
        return rollup.of(self.v, self.repo, self.oid)

    @property
    def repository(self):
        return repository_of(self.v, self.repo)


class RefObj(Obj):
    typename = "Ref"

    def __init__(self, v, repo, ref: str, sha: str):
        super().__init__(v)
        self.repo, self.ref, self.sha, self.key = repo, ref, sha, f"{repo['id']}:{ref}"
        self.prefix, self.name = (HEADS, ref[len(HEADS):]) if ref.startswith(HEADS) else ("refs/", ref[5:])

    @property
    def target(self):
        return CommitObj(self.v, self.repo, self.sha)

    @property
    def repository(self):
        return repository_of(self.v, self.repo)
