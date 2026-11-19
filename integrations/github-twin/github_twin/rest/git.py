"""The Git database: refs, trees, blobs and commits, for clients that write without a clone."""
import base64
import zlib

from .. import events
from ..gitcmd import GitError
from ..repos import ZERO, full_name
from ..users import node_id
from . import shapes
from .api import ApiError, created, invalid, no_content, not_found


def _ref_json(call, repo, ref: str, sha: str) -> dict:
    fn = full_name(repo)
    return {"ref": ref, "node_id": node_id("Ref", zlib.crc32(ref.encode())), "url": call.urls.repo_api(fn, f"/git/{ref}"),
            "object": {"sha": sha, "type": call.repos.object_type(fn, sha) or "commit",
                       "url": call.urls.repo_api(fn, f"/git/commits/{sha}")}}


def _full(ref: str) -> str:
    return ref if ref.startswith("refs/") else f"refs/{ref}"


def get_ref(call):
    repo, ref = call.repo(), _full(call.req.params["ref"])
    sha = call.repos.refs(full_name(repo)).get(ref)
    if sha is None:
        raise not_found()
    return _ref_json(call, repo, ref, sha)


def _move(call, repo, ref: str, new: str, old: str | None) -> None:
    fn = full_name(repo)
    move = (lambda: call.repos.delete_ref(fn, ref)) if new == ZERO else (lambda: call.repos.update_ref(fn, ref, new, old))
    try:
        events.moved(call.ctx, repo, move)
    except GitError as e:
        raise invalid(str(e)) from None


def create_ref(call):
    repo, body = call.repo(), call.body()
    ref, sha = body.get("ref", ""), body.get("sha", "")
    if not ref.startswith("refs/") or ref.count("/") < 2:
        raise invalid("Reference name is invalid")
    fn = full_name(repo)
    if ref in call.repos.refs(fn):
        raise invalid("Reference already exists")
    if call.repos.object_type(fn, sha) is None:
        raise invalid("Object does not exist")
    _move(call, repo, ref, sha, ZERO)
    return created(_ref_json(call, repo, ref, sha))


def update_ref(call):
    repo, ref, body = call.repo(), _full(call.req.params["ref"]), call.body()
    fn, sha = full_name(repo), body.get("sha", "")
    old = call.repos.refs(fn).get(ref)
    if old is None:
        raise invalid("Reference does not exist")
    if call.repos.object_type(fn, sha) is None:
        raise invalid("Object does not exist")
    if not body.get("force") and not call.repos.is_ancestor(fn, old, sha):
        raise invalid("Update is not a fast forward")
    _move(call, repo, ref, sha, old)
    return _ref_json(call, repo, ref, sha)


def delete_ref(call):
    repo, ref = call.repo(), _full(call.req.params["ref"])
    if ref not in call.repos.refs(full_name(repo)):
        raise invalid("Reference does not exist")
    _move(call, repo, ref, ZERO, None)
    return no_content()


def _tree_json(call, repo, sha: str, entries: list) -> dict:
    fn = full_name(repo)
    items = [{"path": e.path, "mode": e.mode, "type": e.type, "sha": e.sha,
              "url": call.urls.repo_api(fn, f"/git/{'trees' if e.type == 'tree' else 'blobs'}/{e.sha}"),
              **({"size": e.size} if e.size is not None else {})} for e in entries]
    return {"sha": sha, "url": call.urls.repo_api(fn, f"/git/trees/{sha}"), "tree": items, "truncated": False}


def _tree_sha(call, repo, treeish: str) -> str:
    out = call.repos.git(full_name(repo), "rev-parse", "--verify", "--quiet", f"{treeish}^{{tree}}", check=False).strip()
    if not out:
        raise not_found()
    return out


def get_tree(call):
    repo = call.repo()
    sha = _tree_sha(call, repo, call.req.params["tree_sha"])
    recursive = call.req.arg("recursive") not in (None, "", "0", "false")
    return _tree_json(call, repo, sha, call.repos.tree(full_name(repo), sha, recursive=recursive))


def _blob_data(body: dict) -> bytes:
    content = body.get("content")
    if not isinstance(content, str):
        raise invalid("Invalid request.\n\n\"content\" wasn't supplied.")
    return base64.b64decode(content) if body.get("encoding") == "base64" else content.encode()


def create_blob(call):
    repo = call.repo()
    sha = call.repos.write_blob(full_name(repo), _blob_data(call.body()))
    return created({"sha": sha, "url": call.urls.repo_api(full_name(repo), f"/git/blobs/{sha}")})


def _edit(call, repo, item: dict) -> tuple:
    if "content" in item and item["content"] is not None:
        return item.get("mode", "100644"), call.repos.write_blob(full_name(repo), item["content"].encode()), item["path"]
    return item.get("mode", "100644"), item.get("sha"), item["path"]


def create_tree(call):
    repo, body = call.repo(), call.body()
    base = _tree_sha(call, repo, body["base_tree"]) if body.get("base_tree") else None
    try:
        sha = call.repos.edit_tree(full_name(repo), base, [_edit(call, repo, i) for i in body.get("tree", [])])
    except (GitError, KeyError) as e:
        raise invalid(f"GitRPC::BadObjectState: {e}") from None
    return created(_tree_json(call, repo, sha, call.repos.tree(full_name(repo), sha)))


def create_commit(call):
    repo, body = call.repo(), call.body()
    fn = full_name(repo)
    tree = _tree_sha(call, repo, body.get("tree", ""))
    parents = [p for p in body.get("parents", []) if call.repos.object_type(fn, p) == "commit"]
    if len(parents) != len(body.get("parents", [])):
        raise invalid("Parent SHA does not exist or is not a commit object")
    author = call.person(body.get("author"))
    sha = call.repos.commit_tree(fn, tree, parents, body.get("message", ""), author,
                                 call.person(body.get("committer") or body.get("author")))
    return created(shapes.git_commit(call, repo, call.repos.commit_info(fn, sha)))


def register(api) -> None:
    api.route("GET", "/repos/{owner}/{repo}/git/ref/{ref:path}")(get_ref)
    api.route("POST", "/repos/{owner}/{repo}/git/refs")(create_ref)
    api.route("PATCH", "/repos/{owner}/{repo}/git/refs/{ref:path}")(update_ref)
    api.route("DELETE", "/repos/{owner}/{repo}/git/refs/{ref:path}")(delete_ref)
    api.route("GET", "/repos/{owner}/{repo}/git/trees/{tree_sha}")(get_tree)
    api.route("POST", "/repos/{owner}/{repo}/git/blobs")(create_blob)
    api.route("POST", "/repos/{owner}/{repo}/git/trees")(create_tree)
    api.route("POST", "/repos/{owner}/{repo}/git/commits")(create_commit)
