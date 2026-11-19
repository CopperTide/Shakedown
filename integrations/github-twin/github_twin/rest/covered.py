"""The REST operations the twin serves, each `(METHOD, path)` exactly as the GHES description names
it; the schema tests check every answer against it and that the router serves exactly these."""

PREFIX = "/api/v3"

COVERED = [
    ("GET", "/"),
    ("GET", "/meta"),
    ("GET", "/rate_limit"),
    ("GET", "/user"),
    ("GET", "/users/{username}"),
    ("GET", "/repos/{owner}/{repo}"),
    ("GET", "/repos/{owner}/{repo}/branches"),
    ("GET", "/repos/{owner}/{repo}/branches/{branch}"),
    ("GET", "/repos/{owner}/{repo}/contents/{path}"),
    ("PUT", "/repos/{owner}/{repo}/contents/{path}"),
    ("DELETE", "/repos/{owner}/{repo}/contents/{path}"),
    ("GET", "/repos/{owner}/{repo}/commits"),
    ("GET", "/repos/{owner}/{repo}/commits/{ref}"),
    ("GET", "/repos/{owner}/{repo}/compare/{basehead}"),
    ("GET", "/repos/{owner}/{repo}/git/ref/{ref}"),
    ("POST", "/repos/{owner}/{repo}/git/refs"),
    ("PATCH", "/repos/{owner}/{repo}/git/refs/{ref}"),
    ("DELETE", "/repos/{owner}/{repo}/git/refs/{ref}"),
    ("GET", "/repos/{owner}/{repo}/git/trees/{tree_sha}"),
    ("POST", "/repos/{owner}/{repo}/git/blobs"),
    ("POST", "/repos/{owner}/{repo}/git/trees"),
    ("POST", "/repos/{owner}/{repo}/git/commits"),
]


def router_ops(router) -> list[tuple[str, str]]:
    """The REST operations a router serves, named as in the description (`{x:path}` → `{x}`)."""
    return [(r.method, r.pattern[len(PREFIX):].replace(":path}", "}")) for r in router.routes
            if r.pattern.startswith(PREFIX + "/")]
