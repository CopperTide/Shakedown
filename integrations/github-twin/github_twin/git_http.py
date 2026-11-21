"""Git smart HTTP by `git http-backend` (CGI): discovery, upload-pack and receive-pack, v0 and v2."""
import subprocess
import threading
from pathlib import Path

from shakedown_part.twin import Response

from .gitcmd import clean_env

CHALLENGE = {"WWW-Authenticate": 'Basic realm="GitHub"'}
CHUNK = 64 * 1024


def challenge() -> Response:
    return Response(401, "Authentication failed\n", headers=dict(CHALLENGE))


def _cgi_env(req, repos_root: Path, path_info: str, user: str) -> dict:
    env = {"GIT_PROJECT_ROOT": str(repos_root), "GIT_HTTP_EXPORT_ALL": "1", "PATH_INFO": path_info,
           "QUERY_STRING": "&".join(f"{k}={v}" for k, vs in req.query.items() for v in vs),
           "REQUEST_METHOD": req.method, "CONTENT_TYPE": req.headers.get("content-type", ""),
           "CONTENT_LENGTH": str(len(req.body)), "REMOTE_USER": user, "REMOTE_ADDR": "127.0.0.1",
           "GIT_HTTP_MAX_REQUEST_BUFFER": "100M"}
    if req.headers.get("git-protocol"):
        env["GIT_PROTOCOL"] = req.headers["git-protocol"]
    if req.headers.get("content-encoding"):
        env["HTTP_CONTENT_ENCODING"] = req.headers["content-encoding"]
    return env


def _read_head(out) -> tuple[int, dict, bytes]:
    """The CGI header block (`Status:` sets the status); what of the body was read with it."""
    buf = b""
    while b"\r\n\r\n" not in buf and b"\n\n" not in buf:
        more = out.read1(CHUNK) if hasattr(out, "read1") else out.read(CHUNK)
        if not more:
            break
        buf += more
    sep = b"\r\n\r\n" if b"\r\n\r\n" in buf else b"\n\n"
    head, _, rest = buf.partition(sep)
    status, headers = 200, {}
    for line in head.decode("latin-1").splitlines():
        name, _, value = line.partition(":")
        if name.lower() == "status":
            status = int(value.split()[0])
        elif name:
            headers[name.strip()] = value.strip()
    return status, headers, rest


def _body(proc, first: bytes):
    yield first
    while chunk := proc.stdout.read1(CHUNK):
        yield chunk
    proc.wait()


def backend(req, repos_root: Path, path_info: str, user: str) -> Response:
    """Run http-backend on the request; the answer streams its output."""
    proc = subprocess.Popen(["git", "http-backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, env=clean_env(_cgi_env(req, repos_root, path_info, user)))

    def feed():
        try:
            proc.stdin.write(req.body)
        finally:
            proc.stdin.close()

    threading.Thread(target=feed, daemon=True).start()
    status, headers, rest = _read_head(proc.stdout)
    return Response(status, headers=headers, stream=_body(proc, rest))


def handle(req, repos_root: Path, owner: str, repo: str, tail: str, user: str) -> Response:
    """Serve `/{owner}/{repo}[.git]/{tail}`; a missing repo is 404 (auth is checked by the caller)."""
    repo = repo.removesuffix(".git")
    if not (repos_root / owner / f"{repo}.git" / "HEAD").is_file():
        return Response(404, "Repository not found.\n")
    resp = backend(req, repos_root, f"/{owner}/{repo}.git/{tail}", user)
    if tail.endswith("git-upload-pack") and req.method == "POST":
        resp.effect = "read"
    return resp


def read_whole(resp: Response) -> Response:
    """The same answer with its stream read into the body (a push: the refs move before it returns)."""
    if resp.stream is not None:
        resp.body, resp.stream = b"".join(resp.stream), None
    return resp
