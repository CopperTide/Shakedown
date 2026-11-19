"""Repositories, their refs and commits, and the pull requests, issues, labels and users they list."""
from ... import records, users
from ...repos import full_name
from .. import errors
from ..paging import connection
from . import issue, pull_request
from .common import Obj, when
from .git import HEADS, RefObj
from .viewer import actor, by_kind



def _ordered(rows: list, order_by: dict | None) -> list:
    """Newest first unless `orderBy` asks ASC (rows come newest first)."""
    return list(reversed(rows)) if (order_by or {}).get("direction") == "ASC" else rows


class RepositoryObj(Obj):
    typename = "Repository"

    def __init__(self, v, repo):
        super().__init__(v)
        self.repo, self.key = repo, repo["id"]
        self.database_id, self.name, self.description = repo["id"], repo["name"], repo["description"]
        self.name_with_owner = full_name(repo)
        self.is_private = bool(repo["private"])
        self.visibility = "PRIVATE" if self.is_private else "PUBLIC"
        self.viewer_permission, self.viewer_default_merge_method = "ADMIN", "MERGE"
        self.parent = self.template_repository = None

    has_issues_enabled = has_wiki_enabled = has_projects_enabled = True
    merge_commit_allowed = rebase_merge_allowed = squash_merge_allowed = True
    viewer_can_administer = is_in_organization = True

    @property
    def owner(self):
        return actor(self.v, self.repo["owner"])

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(self.name_with_owner)

    @property
    def ssh_url(self) -> str:
        return f"git@{self.v.urls.host}:{self.name_with_owner}.git"

    @property
    def resource_path(self) -> str:
        return f"/{self.name_with_owner}"

    @property
    def created_at(self) -> str:
        return when(self.repo["created_ms"])

    @property
    def updated_at(self) -> str:
        return when(self.repo["updated_ms"])

    @property
    def pushed_at(self) -> str:
        return when(self.repo["pushed_ms"])

    @property
    def is_empty(self) -> bool:
        return not self.v.repos.branches(self.name_with_owner)

    @property
    def default_branch_ref(self):
        return self.ref(HEADS + self.repo["default_branch"])

    def ref(self, qualified_name: str, **_):
        name = qualified_name if qualified_name.startswith("refs/") else HEADS + qualified_name
        sha = self.v.repos.refs(self.name_with_owner, name).get(name)
        return RefObj(self.v, self.repo, name, sha) if sha else None

    def refs(self, ref_prefix: str = HEADS, query: str | None = None, **page) -> dict:
        found = [(r, s) for r, s in self.v.repos.refs(self.name_with_owner, ref_prefix).items()
                 if not query or query in r[len(ref_prefix):]]
        return connection(found, lambda rs: RefObj(self.v, self.repo, *rs), **page)

    def pull_request(self, number: int, **_):
        row = self.v.db.execute(f"{records.PULL} WHERE i.repo_id = ? AND i.number = ?", (self.key, number)).fetchone()
        if row is None:
            raise errors.not_found("a PullRequest", f"with the number of {number}")
        return pull_request.PullRequestObj(self.v, self.repo, row)

    def pull_requests(self, states=None, labels=None, head_ref_name=None, base_ref_name=None, order_by=None,
                      **page) -> dict:
        rows = [r for r in records.pulls(self.v.db, self.key) if pull_request.matches(
            self.v, r, states, labels, head_ref_name, base_ref_name)]
        return connection(_ordered(rows, order_by), lambda r: pull_request.PullRequestObj(self.v, self.repo, r), **page)

    def _issue_row(self, number: int):
        return self.v.db.execute("SELECT * FROM issues WHERE repo_id = ? AND number = ?", (self.key, number)).fetchone()

    def issue(self, number: int, **_):
        row = self._issue_row(number)
        if row is None or row["is_pull"]:
            raise errors.not_found("an Issue", f"with the number of {number}")
        return issue.IssueObj(self.v, self.repo, row)

    def issue_or_pull_request(self, number: int, **_):
        row = self._issue_row(number)
        if row is None:
            raise errors.not_found("an issue or pull request", f"with the number of {number}")
        return issue.issue_or_pull(self.v, self.repo, row)

    def issues(self, states=None, labels=None, filter_by=None, order_by=None, **page) -> dict:
        rows = self.v.db.execute("SELECT * FROM issues WHERE repo_id = ? AND is_pull = 0 ORDER BY number DESC",
                                 (self.key,)).fetchall()
        rows = [r for r in rows if issue.matches(self.v, r, states, labels, filter_by)]
        return connection(_ordered(rows, order_by), lambda r: issue.IssueObj(self.v, self.repo, r), **page)

    def labels(self, query: str | None = None, **page) -> dict:
        rows = self.v.db.execute("SELECT * FROM labels WHERE repo_id = ? ORDER BY name", (self.key,)).fetchall()
        rows = [r for r in rows if not query or query.lower() in r["name"].lower()]
        return connection(rows, lambda r: issue.LabelObj(self.v, self.repo, r), **page)

    def label(self, name: str, **_):
        row = records.label(self.v.db, self.key, name)
        return issue.LabelObj(self.v, self.repo, row) if row else None

    def assignable_users(self, query: str | None = None, **page) -> dict:
        people = [a for a in by_kind(self.v, "User") if not query or query.lower() in a.login.lower()]
        return connection(people, **page)

    mentionable_users = assignable_users
