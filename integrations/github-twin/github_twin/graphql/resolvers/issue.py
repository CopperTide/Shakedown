"""Issues, and what issues and pull requests share: labels, assignees, comments, author, state."""
from ... import records
from ...repos import full_name
from ..paging import connection
from .common import Obj, repository_of, upper, when
from .viewer import actor


class LabelObj(Obj):
    typename = "Label"

    def __init__(self, v, repo, row):
        super().__init__(v)
        self.repo, self.row, self.key = repo, row, row["id"]
        self.name, self.color, self.description = row["name"], row["color"], row["description"]
        self.is_default = False

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(full_name(self.repo), f"/labels/{self.name}")

    @property
    def repository(self):
        return repository_of(self.v, self.repo)


class CommentObj(Obj):
    typename = "IssueComment"

    def __init__(self, v, repo, issue_row, row):
        super().__init__(v)
        self.repo, self.issue_row, self.row, self.key = repo, issue_row, row, row["id"]
        self.database_id, self.body, self.body_text = row["id"], row["body"], row["body"]
        self.created_at, self.updated_at = when(row["created_ms"]), when(row["updated_ms"])
        self.author_association, self.reaction_groups = "MEMBER", []

    @property
    def author(self):
        return actor(self.v, self.row["user"])

    @property
    def viewer_did_author(self) -> bool:
        return self.row["user"] == self.v.login

    @property
    def url(self) -> str:
        n, path = self.issue_row["number"], "pull" if self.issue_row["is_pull"] else "issues"
        return self.v.urls.repo_html(full_name(self.repo), f"/{path}/{n}#issuecomment-{self.key}")

    @property
    def issue(self):
        return issue_or_pull(self.v, self.repo, self.issue_row)

    @property
    def pull_request(self):
        return issue_or_pull(self.v, self.repo, self.issue_row) if self.issue_row["is_pull"] else None


class IssueLike(Obj):
    """The fields issues and pull requests have alike, over an issues row (`row`)."""
    PATH = "issues"

    def __init__(self, v, repo, row):
        super().__init__(v)
        self.repo, self.row, self.key = repo, row, row["id"]
        self.database_id = self.full_database_id = row["id"]
        self.number, self.title, self.locked = row["number"], row["title"], bool(row["locked"])
        self.author_association, self.reaction_groups = "MEMBER", []

    @property
    def body(self) -> str:
        return self.row["body"] or ""

    body_text = body

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(full_name(self.repo), f"/{self.PATH}/{self.number}")

    permalink = url

    @property
    def resource_path(self) -> str:
        return f"/{full_name(self.repo)}/{self.PATH}/{self.number}"

    @property
    def closed(self) -> bool:
        return self.row["state"] == "closed"

    @property
    def created_at(self) -> str | None:
        return when(self.row["created_ms"])

    published_at = created_at

    @property
    def updated_at(self) -> str | None:
        return when(self.row["updated_ms"])

    @property
    def closed_at(self) -> str | None:
        return when(self.row["closed_ms"])

    @property
    def author(self):
        return actor(self.v, self.row["user"])

    @property
    def viewer_did_author(self) -> bool:
        return self.row["user"] == self.v.login

    @property
    def repository(self):
        return repository_of(self.v, self.repo)

    def labels(self, **page) -> dict:
        rows = records.labels_of(self.v.db, self.key)
        return connection(rows, lambda r: LabelObj(self.v, self.repo, r), **page)

    def assignees(self, **page) -> dict:
        return connection(records.assignees_of(self.v.db, self.key), lambda x: actor(self.v, x), **page)

    def comments(self, **page) -> dict:
        rows = self.v.db.execute("SELECT * FROM comments WHERE issue_id = ? ORDER BY id", (self.key,)).fetchall()
        return connection(rows, lambda r: CommentObj(self.v, self.repo, self.row, r), **page)

    def participants(self, **page) -> dict:
        logins = sorted({self.row["user"], *records.assignees_of(self.v.db, self.key)})
        return connection(logins, lambda x: actor(self.v, x), **page)


REASONS = ("completed", "not_planned", "reopened")


class IssueObj(IssueLike):
    typename = "Issue"

    @property
    def state(self) -> str:
        return self.row["state"].upper()

    @property
    def state_reason(self) -> str | None:
        reason = self.row["state_reason"]
        return upper(reason) if reason in REASONS else None


def issue_or_pull(v, repo, row):
    """The issue or pull request an issues row is."""
    if row["is_pull"]:
        from .pull_request import PullRequestObj
        return PullRequestObj(v, repo, records.pull(v.db, repo["id"], row["number"]))
    return IssueObj(v, repo, row)


def _labelled(v, row, labels) -> bool:
    return not labels or set(labels) <= {x["name"] for x in records.labels_of(v.db, row["id"])}


def matches(v, row, states, labels, filter_by) -> bool:
    """Whether an issue passes `issues(states, labels, filterBy)`."""
    f = filter_by or {}
    wanted = states or f.get("states")
    if wanted and row["state"].upper() not in wanted:
        return False
    if not _labelled(v, row, labels) or not _labelled(v, row, f.get("labels")):
        return False
    if f.get("assignee") and f["assignee"] not in records.assignees_of(v.db, row["id"]):
        return False
    if f.get("createdBy") and row["user"] != f["createdBy"]:
        return False
    return not f.get("mentioned") or f"@{f['mentioned']}" in (row["body"] or "")
