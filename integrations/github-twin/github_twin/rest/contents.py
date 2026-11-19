"""Files by path: read a file or a folder, and create, update or delete a file in one commit."""
import base64
import binascii

from arena_part.twin import Response

from .. import events
from ..gitcmd import GitError
from ..repos import full_name
from . import shapes
from .api import ApiError, invalid, not_found

FILE_MODE = "100644"
KINDS = {"blob": "file", "tree": "dir", "commit": "submodule"}


def _entry(call, repo, ref: str, e) -> dict:
    fn, u = full_name(repo), call.urls
    kind = "symlink" if e.mode == "120000" else KINDS[e.type]
    git = u.repo_api(fn, f"/git/{'trees' if e.type == 'tree' else 'blobs'}/{e.sha}")
    html = u.repo_html(fn, f"/{'tree' if e.type == 'tree' else 'blob'}/{ref}/{e.path}")
    api = u.repo_api(fn, f"/contents/{e.path}?ref={ref}")
    return {"type": kind, "name": e.path.rsplit("/", 1)[-1], "path": e.path, "sha": e.sha, "size": e.size or 0,
            "url": api, "git_url": git, "html_url": html,
            "download_url": u.repo_html(fn, f"/raw/{ref}/{e.path}") if e.type == "blob" else None,
            "_links": {"self": api, "git": git, "html": html}}


def _commit_of(call, repo, ref: str | None) -> tuple[str, str]:
    ref = ref or repo["default_branch"]
    sha = call.repos.ref(full_name(repo), ref)
    if sha is None:
        raise ApiError(404, f"No commit found for the ref {ref}")
    return ref, sha


def _at(call, repo, commit: str, path: str):
    found = [e for e in call.repos.tree(full_name(repo), commit, path) if e.path == path]
    return found[0] if found else None


def get_contents(call):
    repo, path = call.repo(), call.req.params["path"].strip("/")
    ref, commit = _commit_of(call, repo, call.req.arg("ref"))
    if not path:
        return [_entry(call, repo, ref, e) for e in call.repos.tree(full_name(repo), commit)]
    e = _at(call, repo, commit, path)
    if e is None:
        raise not_found()
    if e.type == "tree":
        return [_entry(call, repo, ref, c) for c in call.repos.tree(full_name(repo), commit, path + "/")]
    out = _entry(call, repo, ref, e)
    data = call.repos.blob(full_name(repo), e.sha)
    return {**out, "encoding": "base64", "content": base64.encodebytes(data).decode()}


def _check_sha(path: str, existing, given: str | None, required: bool) -> None:
    if existing is None and given is None:
        if required:
            raise not_found()
        return
    if existing is not None and given is None:
        raise invalid('Invalid request.\n\n"sha" wasn\'t supplied.')
    if existing is None or existing.sha != given:
        raise ApiError(409, f"{path} does not match {given}")


def _write(call, repo, branch: str, head: str, edit: tuple, body: dict) -> str:
    """Commit `edit` (mode, blob or None, path) on `branch` from `head`; the new commit."""
    fn = full_name(repo)
    tree = call.repos.edit_tree(fn, call.repos.commit_info(fn, head).tree, [edit])
    author = call.person(body.get("author"))
    committer = call.person(body.get("committer") or body.get("author"))
    sha = call.repos.commit_tree(fn, tree, [head], body.get("message", ""), author, committer)
    try:
        events.moved(call.ctx, repo, lambda: call.repos.update_ref(fn, f"refs/heads/{branch}", sha, head))
    except GitError:
        raise ApiError(409, f"is at {call.repos.ref(fn, branch)} but expected {head}") from None
    return sha


def _file_commit(call, repo, sha: str, content) -> dict:
    return {"content": content, "commit": shapes.git_commit(call, repo, call.repos.commit_info(full_name(repo), sha))}


def put_contents(call):
    repo, path, body = call.repo(), call.req.params["path"].strip("/"), call.body()
    branch, head = _commit_of(call, repo, body.get("branch"))
    existing = _at(call, repo, head, path)
    _check_sha(path, existing, body.get("sha"), required=False)
    try:
        data = base64.b64decode(body.get("content", ""), validate=False)
    except (binascii.Error, ValueError):
        raise invalid("content is not valid Base64") from None
    blob = call.repos.write_blob(full_name(repo), data)
    sha = _write(call, repo, branch, head, (FILE_MODE, blob, path), body)
    content = _entry(call, repo, branch, _at(call, repo, sha, path))
    return Response(201 if existing is None else 200, _file_commit(call, repo, sha, content))


def delete_contents(call):
    repo, path, body = call.repo(), call.req.params["path"].strip("/"), call.body()
    branch, head = _commit_of(call, repo, body.get("branch"))
    _check_sha(path, _at(call, repo, head, path), body.get("sha"), required=True)
    sha = _write(call, repo, branch, head, (FILE_MODE, None, path), body)
    return _file_commit(call, repo, sha, None)


def register(api) -> None:
    api.route("GET", "/repos/{owner}/{repo}/contents/{path:path}")(get_contents)
    api.route("PUT", "/repos/{owner}/{repo}/contents/{path:path}")(put_contents)
    api.route("DELETE", "/repos/{owner}/{repo}/contents/{path:path}")(delete_contents)
