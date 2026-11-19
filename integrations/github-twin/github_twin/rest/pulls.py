"""Pull requests: open, list, read (JSON, or the diff and patch media types), edit, merge, files, commits."""
from arena_part.twin import Response

from .. import issue_rules, pull_rules, records
from ..diffs import changed_files, full_diff
from ..repos import full_name
from . import pull_shapes, shapes
from .api import created, no_content, not_found, paged

DIFF, PATCH = "application/vnd.github.v3.diff", "application/vnd.github.v3.patch"


def number(call) -> int:
    try:
        return int(call.req.params["pull_number"])
    except ValueError:
        raise not_found() from None


def the_pull(call):
    repo = call.repo()
    return repo, records.pull(call.db, repo["id"], number(call))


def create_pull(call):
    repo, b = call.repo(), call.body()
    pr = pull_rules.open_pull(call, repo, call.login, b.get("head", ""), b.get("base"), b.get("title", ""),
                              b.get("body"), bool(b.get("draft")))
    return created(pull_shapes.full(call, repo, pr))


def _filters(call) -> tuple[str, tuple]:
    state = call.req.arg("state", "open")
    terms = [("i.state = ?", state)] if state in ("open", "closed") else []
    terms += [(f"{col} = ?", call.req.arg(key).split(":", 1)[-1])
              for key, col in (("head", "p.head_ref"), ("base", "p.base_ref")) if call.req.arg(key)]
    return " AND ".join(t for t, _ in terms) or "1", tuple(a for _, a in terms)


def list_pulls(call):
    repo = call.repo()
    where, args = _filters(call)
    rows = records.pulls(call.db, repo["id"], where, args)
    if call.req.arg("direction") == "asc":
        rows.reverse()
    resp = paged(call, rows)
    resp.body = [pull_shapes.simple(call, repo, pr) for pr in resp.body]
    return resp


def _raw(call, repo, pr, media: str) -> Response:
    fn, base = full_name(repo), pull_shapes.base_sha(call, repo, pr)
    mb = call.repos.merge_base(fn, base, pr["head_sha"]) or base
    if media == DIFF:
        text = full_diff(call.repos, fn, mb, pr["head_sha"])
    else:
        text = call.repos.git(fn, "format-patch", "--stdout", f"{mb}..{pr['head_sha']}")
    return Response(200, text, headers={"Content-Type": f"{media}; charset=utf-8"})


def get_pull(call):
    repo, pr = the_pull(call)
    accept = call.req.headers.get("accept", "")
    for media in (DIFF, PATCH):
        if media in accept:
            return _raw(call, repo, pr, media)
    return pull_shapes.full(call, repo, pr)


def update_pull(call):
    repo, pr = the_pull(call)
    b = call.body()
    fields = {k: b[k] for k in ("title", "body") if k in b}
    if fields:
        records.touch(call.db, pr["id"], call.now(), **fields)
    if b.get("base"):
        records.set_pull(call.db, pr["id"], base_ref=b["base"])
    if b.get("state") in ("open", "closed"):
        issue_rules.set_state(call, repo, records.issue(call.db, repo["id"], pr["number"]), b["state"], call.login)
    return pull_shapes.full(call, repo, records.pull(call.db, repo["id"], pr["number"]))


def merge_pull(call):
    repo, pr = the_pull(call)
    b = call.body()
    sha = pull_rules.merge(call, repo, pr, call.login, b.get("merge_method", "merge"), b.get("commit_title"),
                           b.get("commit_message"), b.get("sha"))
    return {"sha": sha, "merged": True, "message": "Pull Request successfully merged"}


def is_merged(call):
    _, pr = the_pull(call)
    if not pr["merged"]:
        raise not_found()
    return no_content()


def pull_files(call):
    repo, pr = the_pull(call)
    fn, base = full_name(repo), pull_shapes.base_sha(call, repo, pr)
    mb = call.repos.merge_base(fn, base, pr["head_sha"]) or base
    resp = paged(call, changed_files(call.repos, fn, mb, pr["head_sha"]))
    resp.body = [shapes.diff_entry(call, repo, pr["head_sha"], f) for f in resp.body]
    return resp


def pull_commits(call):
    repo, pr = the_pull(call)
    fn, base = full_name(repo), pull_shapes.base_sha(call, repo, pr)
    shas = list(reversed(call.repos.rev_list(fn, f"{base}..{pr['head_sha']}")))
    resp = paged(call, shas)
    resp.body = [shapes.commit(call, repo, call.repos.commit_info(fn, s)) for s in resp.body]
    return resp


def register(api) -> None:
    p = "/repos/{owner}/{repo}/pulls"
    api.route("POST", p)(create_pull)
    api.route("GET", p)(list_pulls)
    api.route("GET", p + "/{pull_number}")(get_pull)
    api.route("PATCH", p + "/{pull_number}")(update_pull)
    api.route("PUT", p + "/{pull_number}/merge")(merge_pull)
    api.route("GET", p + "/{pull_number}/merge")(is_merged)
    api.route("GET", p + "/{pull_number}/files")(pull_files)
    api.route("GET", p + "/{pull_number}/commits")(pull_commits)
