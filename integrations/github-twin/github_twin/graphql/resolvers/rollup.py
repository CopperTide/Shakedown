"""A commit's status check rollup: its check runs and the latest status of each context, combined
as GitHub combines them (`StatusCheckRollup.state`, the `StatusCheckRollupContext` union)."""
from ... import ci, users
from ..paging import connection
from .common import Obj, upper, when
from .viewer import actor

STATES = {"pending": "PENDING", "success": "SUCCESS", "failure": "FAILURE"}


class CheckRunObj(Obj):
    typename = "CheckRun"

    def __init__(self, v, repo, row):
        super().__init__(v)
        self.repo, self.row, self.key = repo, row, row["id"]
        self.database_id, self.name, self.title = row["id"], row["name"], row["title"]
        self.summary, self.text = row["summary"], row["text"]
        self.status, self.conclusion = row["status"].upper(), upper(row["conclusion"])
        self.started_at, self.completed_at = when(row["started_ms"]), when(row["completed_ms"])
        self.details_url = row["details_url"]

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(f"{self.repo['owner']}/{self.repo['name']}", f"/runs/{self.key}")

    permalink = url

    @property
    def check_suite(self) -> dict:
        return {"id": users.node_id("CheckSuite", self.row["suite_id"]), "databaseId": self.row["suite_id"],
                "workflowRun": None, "app": None, "status": self.status, "conclusion": self.conclusion}

    def is_required(self, **_) -> bool:
        return False


class StatusContextObj(Obj):
    typename = "StatusContext"

    def __init__(self, v, repo, row):
        super().__init__(v)
        self.repo, self.row, self.key = repo, row, row["id"]
        self.context, self.state, self.description = row["context"], row["state"].upper(), row["description"]
        self.target_url, self.created_at = row["target_url"], when(row["created_ms"])

    @property
    def creator(self):
        return actor(self.v, self.row["creator"])

    def avatar_url(self, **_) -> str:
        return self.v.urls.html("/avatars/u/0")

    def is_required(self, **_) -> bool:
        return False


class RollupObj(Obj):
    typename = "StatusCheckRollup"

    def __init__(self, v, repo, sha: str, state: str):
        super().__init__(v)
        self.repo, self.sha, self.key, self.state = repo, sha, f"{repo['id']}:{sha}", STATES[state]

    def contexts(self, **page) -> dict:
        db, rid = self.v.db, self.repo["id"]
        items = [CheckRunObj(self.v, self.repo, r) for r in ci.runs(db, rid, self.sha)]
        items += [StatusContextObj(self.v, self.repo, s) for s in reversed(ci.latest_statuses(db, rid, self.sha))]
        return connection(items, **page)


def of(v, repo, sha: str) -> RollupObj | None:
    """The rollup of a commit; None when nothing reported on it, as on GitHub."""
    state = ci.head_checks(v.db, repo["id"], sha)
    return None if state == "none" else RollupObj(v, repo, sha, state)
