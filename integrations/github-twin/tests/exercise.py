"""One sample request per covered operation, for the schema test: `exercise(twin, op, tmp)`
answers `(status, body)`."""
from conftest import api, api_raw, b64

R = "/api/v3/repos/acme/app"


def _main(t):
    return api(t, "GET", f"{R}/branches/main")["commit"]["sha"]


def _tree(t):
    return api(t, "GET", f"{R}/commits/{_main(t)}")["commit"]["tree"]["sha"]


def _file_sha(t, path):
    return api(t, "GET", f"{R}/contents/{path}")["sha"]


def _topic(t):
    api(t, "POST", f"{R}/git/refs", {"ref": "refs/heads/topic", "sha": _main(t)})


def _blob(t):
    return api(t, "POST", f"{R}/git/blobs", {"content": "x\n"})["sha"]


SAMPLES = {
    ("GET", "/"): lambda t: ("GET", "/api/v3/", None),
    ("GET", "/meta"): lambda t: ("GET", "/api/v3/meta", None),
    ("GET", "/rate_limit"): lambda t: ("GET", "/api/v3/rate_limit", None),
    ("GET", "/user"): lambda t: ("GET", "/api/v3/user", None),
    ("GET", "/users/{username}"): lambda t: ("GET", "/api/v3/users/coder", None),
    ("GET", "/repos/{owner}/{repo}"): lambda t: ("GET", R, None),
    ("GET", "/repos/{owner}/{repo}/branches"): lambda t: ("GET", f"{R}/branches", None),
    ("GET", "/repos/{owner}/{repo}/branches/{branch}"): lambda t: ("GET", f"{R}/branches/main", None),
    ("GET", "/repos/{owner}/{repo}/contents/{path}"): lambda t: ("GET", f"{R}/contents/app.py", None),
    ("PUT", "/repos/{owner}/{repo}/contents/{path}"): lambda t: (
        "PUT", f"{R}/contents/app.py", {"message": "Edit", "content": b64("y\n"), "sha": _file_sha(t, "app.py")}),
    ("DELETE", "/repos/{owner}/{repo}/contents/{path}"): lambda t: (
        "DELETE", f"{R}/contents/app.py", {"message": "Remove", "sha": _file_sha(t, "app.py")}),
    ("GET", "/repos/{owner}/{repo}/commits"): lambda t: ("GET", f"{R}/commits", None),
    ("GET", "/repos/{owner}/{repo}/commits/{ref}"): lambda t: ("GET", f"{R}/commits/main", None),
    ("GET", "/repos/{owner}/{repo}/compare/{basehead}"): lambda t: (_topic(t), ("GET", f"{R}/compare/main...topic", None))[1],
    ("GET", "/repos/{owner}/{repo}/git/ref/{ref}"): lambda t: ("GET", f"{R}/git/ref/heads/main", None),
    ("POST", "/repos/{owner}/{repo}/git/refs"): lambda t: ("POST", f"{R}/git/refs", {"ref": "refs/heads/n", "sha": _main(t)}),
    ("PATCH", "/repos/{owner}/{repo}/git/refs/{ref}"): lambda t: (_topic(t), ("PATCH", f"{R}/git/refs/heads/topic", {"sha": _main(t)}))[1],
    ("DELETE", "/repos/{owner}/{repo}/git/refs/{ref}"): lambda t: (_topic(t), ("DELETE", f"{R}/git/refs/heads/topic", None))[1],
    ("GET", "/repos/{owner}/{repo}/git/trees/{tree_sha}"): lambda t: ("GET", f"{R}/git/trees/{_tree(t)}?recursive=1", None),
    ("POST", "/repos/{owner}/{repo}/git/blobs"): lambda t: ("POST", f"{R}/git/blobs", {"content": "x\n"}),
    ("POST", "/repos/{owner}/{repo}/git/trees"): lambda t: ("POST", f"{R}/git/trees", {
        "base_tree": _tree(t), "tree": [{"path": "x.txt", "mode": "100644", "type": "blob", "sha": _blob(t)}]}),
    ("POST", "/repos/{owner}/{repo}/git/commits"): lambda t: ("POST", f"{R}/git/commits", {
        "message": "M", "tree": _tree(t), "parents": [_main(t)]}),
}


def exercise(twin, op, tmp_path):
    method, path, body = SAMPLES[op](twin)
    r = api_raw(twin, method, path, body)
    return r.status, r.json()
