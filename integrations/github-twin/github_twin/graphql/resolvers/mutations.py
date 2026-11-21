"""The mutation root: the pull request and issue mutations gh sends (research §2.4, §5.1), each by the
same rules as REST. A read-only agent's mutation is refused; one that fails changes nothing."""
from functools import wraps

from ... import issue_rules, pull_rules, records
from .. import errors
from . import nodes
from .issue import CommentObj, issue_or_pull
from .pull_request import ReviewObj


def mutation(fn):
    """Refuse a read-only caller; run in a savepoint; answer with the input's `clientMutationId`."""

    @wraps(fn)
    def run(self, input: dict, **_):
        if self.v.readonly:
            raise errors.forbidden()
        with self.v.ctx.savepoint("gql"):
            out = fn(self, input)
        self.v.wrote = True
        return {**out, "clientMutationId": input.get("clientMutationId")}

    return run


def _given(input: dict, *keys: str) -> dict:
    return {k: input[k] for k in keys if input.get(k) is not None}


class Mutation:
    def __init__(self, v):
        self.v = v

    # --- lookups ------------------------------------------------------------------------
    def _pr(self, node_id: str):
        return nodes.need(self.v, node_id, "PullRequest")

    def _subject(self, node_id: str):
        return nodes.need(self.v, node_id, "Issue", "PullRequest")

    def _logins(self, ids) -> list:
        return [nodes.need(self.v, i, "User", "Bot").login for i in ids or ()]

    def _labels(self, ids) -> list:
        return [nodes.need(self.v, i, "Label").name for i in ids or ()]

    def _fresh(self, obj):
        return issue_or_pull(self.v, obj.repo, records.issue(self.v.db, obj.repo["id"], obj.number))

    def _edit(self, obj, input: dict) -> None:
        changes = _given(input, "title", "body")
        if input.get("state"):
            changes["state"] = input["state"].lower()
        if input.get("labelIds") is not None:
            changes["labels"] = self._labels(input["labelIds"])
        if input.get("assigneeIds") is not None:
            changes["assignees"] = self._logins(input["assigneeIds"])
        issue_rules.edit(self.v, obj.repo, obj.row, self.v.login, changes)

    # --- pull requests ------------------------------------------------------------------
    @mutation
    def create_pull_request(self, input: dict) -> dict:
        repo = nodes.need(self.v, input["repositoryId"], "Repository").repo
        pr = pull_rules.open_pull(self.v, repo, self.v.login, input["headRefName"], input["baseRefName"],
                                  input.get("title", ""), input.get("body"), bool(input.get("draft")))
        if input.get("maintainerCanModify") is False:
            records.set_pull(self.v.db, pr["id"], maintainer_can_modify=0)
        return {"pullRequest": issue_or_pull(self.v, repo, pr)}

    @mutation
    def update_pull_request(self, input: dict) -> dict:
        pr = self._pr(input["pullRequestId"])
        if input.get("baseRefName"):
            records.set_pull(self.v.db, pr.key, base_ref=input["baseRefName"])
        if input.get("maintainerCanModify") is not None:
            records.set_pull(self.v.db, pr.key, maintainer_can_modify=int(input["maintainerCanModify"]))
        self._edit(pr, input)
        return {"pullRequest": self._fresh(pr)}

    @mutation
    def merge_pull_request(self, input: dict) -> dict:
        pr = self._pr(input["pullRequestId"])
        pull_rules.merge(self.v, pr.repo, pr.row, self.v.login, (input.get("mergeMethod") or "MERGE").lower(),
                         input.get("commitHeadline"), input.get("commitBody"), input.get("expectedHeadOid"))
        return {"pullRequest": self._fresh(pr)}

    def _set_state(self, input: dict, key: str, state: str, reason: str | None = None) -> dict:
        obj = self._subject(input[key])
        issue_rules.set_state(self.v, obj.repo, obj.row, state, self.v.login, reason)
        return self._fresh(obj)

    @mutation
    def close_pull_request(self, input: dict) -> dict:
        return {"pullRequest": self._set_state(input, "pullRequestId", "closed")}

    @mutation
    def reopen_pull_request(self, input: dict) -> dict:
        return {"pullRequest": self._set_state(input, "pullRequestId", "open")}

    @mutation
    def mark_pull_request_ready_for_review(self, input: dict) -> dict:
        pr = self._pr(input["pullRequestId"])
        records.set_pull(self.v.db, pr.key, draft=0)
        records.touch(self.v.db, pr.key, self.v.now())
        return {"pullRequest": self._fresh(pr)}

    @mutation
    def add_pull_request_review(self, input: dict) -> dict:
        pr = self._pr(input["pullRequestId"])
        row = pull_rules.review(self.v, pr.repo, pr.row, self.v.login, input.get("event"), input.get("body") or "",
                                input.get("commitOID"))
        return {"pullRequestReview": ReviewObj(self.v, pr.repo, pr.row, row)}

    @mutation
    def request_reviews(self, input: dict) -> dict:
        pr = self._pr(input["pullRequestId"])
        logins = self._logins(input.get("userIds"))
        if not input.get("union"):
            gone = [x for x in records.requested(self.v.db, pr.key) if x not in logins]
            pull_rules.remove_reviewers(self.v, pr.row, gone)
        pull_rules.request_reviewers(self.v, pr.repo, pr.row, self.v.login, logins)
        return {"pullRequest": self._fresh(pr)}

    # --- issues and comments ------------------------------------------------------------
    @mutation
    def add_comment(self, input: dict) -> dict:
        obj = self._subject(input["subjectId"])
        row = issue_rules.comment(self.v, obj.repo, obj.row, self.v.login, input.get("body", ""))
        node = CommentObj(self.v, obj.repo, obj.row, row)
        return {"commentEdge": {"node": node, "cursor": node.id}, "subject": self._fresh(obj)}

    @mutation
    def create_issue(self, input: dict) -> dict:
        repo = nodes.need(self.v, input["repositoryId"], "Repository").repo
        row = issue_rules.create(self.v, repo, self.v.login, input.get("title", ""), input.get("body"),
                                 self._labels(input.get("labelIds")), self._logins(input.get("assigneeIds")))
        return {"issue": issue_or_pull(self.v, repo, row)}

    @mutation
    def update_issue(self, input: dict) -> dict:
        obj = nodes.need(self.v, input["id"], "Issue")
        self._edit(obj, input)
        return {"issue": self._fresh(obj)}

    @mutation
    def close_issue(self, input: dict) -> dict:
        reason = (input.get("stateReason") or "COMPLETED").lower()
        return {"issue": self._set_state(input, "issueId", "closed", reason)}

    @mutation
    def reopen_issue(self, input: dict) -> dict:
        return {"issue": self._set_state(input, "issueId", "open")}

    @mutation
    def add_labels_to_labelable(self, input: dict) -> dict:
        obj = self._subject(input["labelableId"])
        records.add_labels(self.v.db, obj.repo["id"], obj.key, self._labels(input.get("labelIds")))
        return {"labelable": self._fresh(obj)}

    @mutation
    def remove_labels_from_labelable(self, input: dict) -> dict:
        obj = self._subject(input["labelableId"])
        ids = [nodes.need(self.v, i, "Label").key for i in input.get("labelIds") or ()]
        self.v.db.executemany("DELETE FROM issue_labels WHERE issue_id = ? AND label_id = ?", [(obj.key, i) for i in ids])
        return {"labelable": self._fresh(obj)}
