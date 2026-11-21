"""Global node ids (`node(id:)`, mutation inputs): the twin's ids are base64 of `04:<Type><key>`, as
its REST answers give them."""
import base64
import binascii
import re

from ... import records
from .. import errors
from . import git, issue, pull_request, repository, rollup
from .viewer import Actor

ID = re.compile(r"04:([A-Z][A-Za-z]*?)(\d.*)")


def decode(node_id: str) -> tuple[str, str] | None:
    try:
        m = ID.fullmatch(base64.b64decode(node_id, validate=True).decode())
    except (binascii.Error, UnicodeDecodeError, ValueError):
        return None
    return (m.group(1), m.group(2)) if m else None


def _row(v, table: str, key: str):
    return v.db.execute(f"SELECT * FROM {table} WHERE id = ?", (key,)).fetchone()


def _repo_of(v, issue_id):
    return v.db.execute("SELECT r.* FROM repos r JOIN issues i ON i.repo_id = r.id WHERE i.id = ?", (issue_id,)).fetchone()


def _issue(v, kind: str, key: str):
    row = _row(v, "issues", key)
    if row is None or bool(row["is_pull"]) != (kind == "PullRequest"):
        return None
    return issue.issue_or_pull(v, _repo_of(v, row["id"]), row)


def _comment(v, kind: str, key: str):
    row = _row(v, "comments", key)
    if row is None:
        return None
    parent = _row(v, "issues", row["issue_id"])
    return issue.CommentObj(v, _repo_of(v, parent["id"]), parent, row)


def _review(v, kind: str, key: str):
    row = _row(v, "reviews", key)
    if row is None:
        return None
    repo = _repo_of(v, row["issue_id"])
    pr = v.db.execute(f"{records.PULL} WHERE i.id = ?", (row["issue_id"],)).fetchone()
    return pull_request.ReviewObj(v, repo, pr, row)


def _label(v, kind: str, key: str):
    row = _row(v, "labels", key)
    return issue.LabelObj(v, _row(v, "repos", row["repo_id"]), row) if row else None


def _commit(v, kind: str, key: str):
    repo_id, _, sha = key.partition(":")
    repo = _row(v, "repos", repo_id)
    return git.CommitObj(v, repo, sha) if repo and v.repos.object_type(
        f"{repo['owner']}/{repo['name']}", sha) == "commit" else None


def _check_run(v, kind: str, key: str):
    row = _row(v, "check_runs", key)
    return rollup.CheckRunObj(v, _row(v, "repos", row["repo_id"]), row) if row else None


def _actor(v, kind: str, key: str):
    row = _row(v, "users", key)
    return Actor(v, row) if row and row["type"] == kind else None


def _repository(v, kind: str, key: str):
    row = _row(v, "repos", key)
    return repository.RepositoryObj(v, row) if row else None


FINDERS = {"User": _actor, "Bot": _actor, "Organization": _actor, "Repository": _repository,
           "PullRequest": _issue, "Issue": _issue, "IssueComment": _comment, "PullRequestReview": _review,
           "Label": _label, "Commit": _commit, "CheckRun": _check_run}


def find(v, node_id: str):
    """The object a global id names; None when it names none."""
    found = decode(node_id or "")
    finder = FINDERS.get(found[0]) if found else None
    return finder(v, *found) if finder else None


def need(v, node_id: str, *types: str):
    """The object a mutation input names, of one of `types`; GitHub's NOT_FOUND otherwise."""
    obj = find(v, node_id)
    if obj is None or types and obj.typename not in types:
        raise errors.no_node(node_id)
    return obj
