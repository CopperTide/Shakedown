"""Pull requests: their branches, merge state, commits, files, reviews and review requests."""
from ... import records
from ...diffs import changed_files
from ...repos import full_name
from ...rest import pull_shapes
from ..paging import connection
from . import rollup
from .common import Obj, repository_of, when
from .git import CommitObj
from .issue import IssueLike, _labelled
from .viewer import actor

MERGE_STATES = {"dirty": "DIRTY", "draft": "DRAFT", "blocked": "BLOCKED", "unstable": "UNSTABLE", "clean": "CLEAN"}
CHANGE_TYPES = {"added": "ADDED", "removed": "DELETED", "modified": "MODIFIED", "renamed": "RENAMED",
                "copied": "COPIED", "changed": "CHANGED"}
DECIDING = ("APPROVED", "CHANGES_REQUESTED", "DISMISSED")


def state_of(row) -> str:
    return "MERGED" if row["merged"] else row["state"].upper()


def matches(v, row, states, labels, head: str | None, base: str | None) -> bool:
    """Whether a pull request passes `pullRequests(states, labels, headRefName, baseRefName)`."""
    if states and state_of(row) not in states:
        return False
    if head and row["head_ref"] != head or base and row["base_ref"] != base:
        return False
    return _labelled(v, row, labels)


class ReviewObj(Obj):
    typename = "PullRequestReview"

    def __init__(self, v, repo, pr, row):
        super().__init__(v)
        self.repo, self.pr, self.row, self.key = repo, pr, row, row["id"]
        self.database_id = self.full_database_id = row["id"]
        self.body = self.body_text = row["body"]
        self.state, self.author_association, self.reaction_groups = row["state"], "MEMBER", []
        self.submitted_at = self.created_at = self.updated_at = when(row["submitted_ms"])

    @property
    def author(self):
        return actor(self.v, self.row["user"])

    @property
    def commit(self):
        return CommitObj(self.v, self.repo, self.row["commit_id"]) if self.row["commit_id"] else None

    @property
    def url(self) -> str:
        return self.v.urls.repo_html(full_name(self.repo), f"/pull/{self.pr['number']}#pullrequestreview-{self.key}")

    @property
    def pull_request(self):
        return PullRequestObj(self.v, self.repo, self.pr)


class PullRequestObj(IssueLike):
    typename = "PullRequest"
    PATH = "pull"

    def __init__(self, v, repo, row):
        super().__init__(v, repo, row)
        self.base_ref_name, self.head_ref_name, self.head_ref_oid = row["base_ref"], row["head_ref"], row["head_sha"]
        self.is_draft, self.merged = bool(row["draft"]), bool(row["merged"])
        self.maintainer_can_modify, self.is_cross_repository = bool(row["maintainer_can_modify"]), False
        self.merged_at = when(row["merged_ms"])
        self.state = state_of(row)

    @property
    def fn(self) -> str:
        return full_name(self.repo)

    @property
    def base_ref_oid(self) -> str:
        return pull_shapes.base_sha(self.v, self.repo, self.row)

    def _merge_base(self) -> str:
        return self.v.repos.merge_base(self.fn, self.base_ref_oid, self.head_ref_oid) or self.base_ref_oid

    @property
    def head_repository(self):
        return repository_of(self.v, self.repo)

    base_repository = head_repository

    @property
    def head_repository_owner(self):
        return actor(self.v, self.repo["owner"])

    @property
    def head_ref(self):
        return repository_of(self.v, self.repo).ref(self.head_ref_name)

    @property
    def base_ref(self):
        return repository_of(self.v, self.repo).ref(self.base_ref_name)

    @property
    def mergeable(self) -> str:
        known, _ = pull_shapes.mergeability(self.v, self.repo, self.row)
        return "UNKNOWN" if known is None else "MERGEABLE" if known else "CONFLICTING"

    @property
    def merge_state_status(self) -> str:
        _, state = pull_shapes.mergeability(self.v, self.repo, self.row)
        return MERGE_STATES.get(state, "UNKNOWN")

    def _numstat(self) -> list:
        out = self.v.repos.git(self.fn, "diff", "--numstat", self._merge_base(), self.head_ref_oid).splitlines()
        return [line.split("\t") for line in out]

    @property
    def additions(self) -> int:
        return sum(int(a) for a, *_ in self._numstat() if a != "-")

    @property
    def deletions(self) -> int:
        return sum(int(d) for _, d, *_ in self._numstat() if d != "-")

    @property
    def changed_files(self) -> int:
        return len(self._numstat())

    def commits(self, **page) -> dict:
        shas = list(reversed(self.v.repos.rev_list(self.fn, f"{self._merge_base()}..{self.head_ref_oid}")))
        return connection(shas, lambda s: {"commit": CommitObj(self.v, self.repo, s), "url": "",
                                           "id": s, "resourcePath": ""}, **page)

    def files(self, **page) -> dict:
        found = changed_files(self.v.repos, self.fn, self._merge_base(), self.head_ref_oid)
        return connection(found, lambda f: {"path": f.filename, "additions": f.additions, "deletions": f.deletions,
                                            "changeType": CHANGE_TYPES.get(f.status, "MODIFIED"),
                                            "viewerViewedState": "UNVIEWED"}, **page)

    def _reviews(self) -> list:
        return [ReviewObj(self.v, self.repo, self.row, r) for r in records.reviews_of(self.v.db, self.key)]

    def _visible(self, r: ReviewObj) -> bool:
        """A pending review is seen by its author only."""
        return r.state != "PENDING" or r.row["user"] == self.v.login

    def reviews(self, states=None, author: str | None = None, **page) -> dict:
        found = [r for r in self._reviews() if self._visible(r) and (not states or r.state in states)
                 and (not author or r.row["user"] == author)]
        return connection(found, **page)

    def _latest(self, kinds) -> list:
        latest = {}
        for r in self._reviews():
            if r.state in kinds:
                latest[r.row["user"]] = r
        return sorted(latest.values(), key=lambda r: r.key)

    def latest_reviews(self, **page) -> dict:
        return connection(self._latest(DECIDING + ("COMMENTED",)), **page)

    def latest_opinionated_reviews(self, **page) -> dict:
        return connection(self._latest(("APPROVED", "CHANGES_REQUESTED")), **page)

    @property
    def review_decision(self) -> str | None:
        """Null unless the base branch requires reviews, as on GitHub."""
        needed = pull_shapes.required_reviews(self.v, self.repo)
        if not needed:
            return None
        states = [r.state for r in self._latest(("APPROVED", "CHANGES_REQUESTED"))]
        if "CHANGES_REQUESTED" in states:
            return "CHANGES_REQUESTED"
        approvals = records.approvals_on(self.v.db, self.key, self.head_ref_oid)
        return "APPROVED" if len(approvals) >= needed else "REVIEW_REQUIRED"

    def review_requests(self, **page) -> dict:
        logins = records.requested(self.v.db, self.key)
        return connection(logins, lambda x: {"id": x, "asCodeOwner": False, "requestedReviewer": actor(self.v, x),
                                             "pullRequest": self}, **page)

    @property
    def merged_by(self):
        return actor(self.v, self.row["merged_by"])

    @property
    def merge_commit(self):
        sha = self.row["merge_commit_sha"]
        return CommitObj(self.v, self.repo, sha) if sha else None

    @property
    def status_check_rollup(self):
        return rollup.of(self.v, self.repo, self.head_ref_oid)

    @property
    def total_comments_count(self) -> int:
        return records.comment_count(self.v.db, self.key) + records.review_comment_count(self.v.db, self.key)

    @property
    def checks_url(self) -> str:
        return self.v.urls.repo_html(self.fn, f"/pull/{self.number}/checks")
