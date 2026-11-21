import base64
import re

from conftest import api, api_raw, b64, commit, git, head, push

URL = re.compile(r"https?://[^\s\"'{}]+")


def urls_in(value):
    if isinstance(value, dict):
        return [u for k, v in value.items() for u in urls_in(v)]
    if isinstance(value, list):
        return [u for v in value for u in urls_in(v)]
    return URL.findall(value) if isinstance(value, str) else []


def test_every_url_uses_the_public_base(twin):   # Review Focus 1
    body = api(twin, "GET", "/api/v3/repos/acme/app", forwarded=("https", "github.acme.test"))
    assert urls_in(body) and all(u.startswith("https://github.acme.test/") for u in urls_in(body))
    assert body["url"] == "https://github.acme.test/api/v3/repos/acme/app"
    assert body["clone_url"] == "https://github.acme.test/acme/app.git"
    assert (body["full_name"], body["default_branch"], body["private"]) == ("acme/app", "main", True)


def test_root_and_meta_tell_gh_what_it_needs(twin):
    r = api_raw(twin, "GET", "/api/v3/")
    assert r.status == 200 and r.headers["x-oauth-scopes"] == "repo, read:org, workflow"
    assert r.headers["x-github-enterprise-version"] == "3.20.0"
    assert r.headers["content-type"] == "application/json; charset=utf-8"
    assert api(twin, "GET", "/api/v3/meta")["installed_version"] == "3.20.0"


def test_the_user_is_the_agent_else_the_default_user(twin):
    assert api(twin, "GET", "/api/v3/user")["login"] == "coder"
    assert api(twin, "GET", "/api/v3/user", as_user="reviewer")["login"] == "reviewer"
    assert api(twin, "GET", "/api/v3/users/reviewer")["login"] == "reviewer"
    assert api_raw(twin, "GET", "/api/v3/users/nobody").status == 404


def test_bad_token_is_bad_credentials(twin):
    r = api_raw(twin, "GET", "/api/v3/user", token="wrong")
    assert (r.status, r.json()["message"]) == (401, "Bad credentials")
    assert r.json()["documentation_url"].startswith("https://docs.github.com/")


def test_unknown_api_paths_are_not_found(twin):
    r = api_raw(twin, "GET", "/api/v3/repos/acme/app/nothing-here")
    assert (r.status, r.json()["message"]) == (404, "Not Found")
    assert api_raw(twin, "GET", "/api/v3/repos/acme/nope").status == 404


def test_branches_and_one_branch(twin):
    [b] = api(twin, "GET", "/api/v3/repos/acme/app/branches")
    assert (b["name"], b["protected"]) == ("main", False) and len(b["commit"]["sha"]) == 40
    one = api(twin, "GET", "/api/v3/repos/acme/app/branches/main")
    assert one["commit"]["sha"] == b["commit"]["sha"]
    assert one["_links"]["html"] == "https://github.acme.test/acme/app/tree/main"


def test_contents_read_a_file_and_a_folder(twin):
    f = api(twin, "GET", "/api/v3/repos/acme/app/contents/app.py")
    assert (f["type"], f["encoding"], f["path"]) == ("file", "base64", "app.py")
    assert base64.b64decode(f["content"]).decode().startswith("def add")
    names = [e["name"] for e in api(twin, "GET", "/api/v3/repos/acme/app/contents/?ref=main")]
    assert names == ["README.md", "app.py", "test_app.py"]


def test_contents_create_update_and_delete(twin, clone):
    made = api_raw(twin, "PUT", "/api/v3/repos/acme/app/contents/docs/new.md",
                   {"message": "Add new", "content": b64("hello\n"), "branch": "main"}, as_user="dana")
    assert made.status == 201
    sha = made.json()["content"]["sha"]
    assert made.json()["commit"]["author"]["name"] == "dana"
    upd = api(twin, "PUT", "/api/v3/repos/acme/app/contents/docs/new.md",
              {"message": "Edit", "content": b64("bye\n"), "sha": sha})
    gone = api(twin, "DELETE", "/api/v3/repos/acme/app/contents/docs/new.md",
               {"message": "Remove", "sha": upd["content"]["sha"]})
    assert gone["content"] is None
    git(clone, "pull", "-q", "origin", "main", creds=twin.token)
    assert head(clone) == gone["commit"]["sha"] and not (clone / "docs/new.md").exists()


def test_contents_put_with_a_stale_sha_is_409(twin):
    stale = "0" * 40
    r = api_raw(twin, "PUT", "/api/v3/repos/acme/app/contents/app.py",
                {"message": "Edit", "content": b64("x\n"), "sha": stale})
    assert (r.status, r.json()["message"]) == (409, f"app.py does not match {stale}")


def test_commits_list_one_and_compare(twin, clone):
    base = head(clone)
    sha = commit(clone, "b.txt", "two\n")
    push(clone, twin.token, "HEAD:refs/heads/feature")
    listed = api(twin, "GET", "/api/v3/repos/acme/app/commits?sha=feature")
    assert [c["sha"] for c in listed] == [sha, base]
    one = api(twin, "GET", f"/api/v3/repos/acme/app/commits/{sha}")
    assert one["url"] == f"https://github.acme.test/api/v3/repos/acme/app/commits/{sha}"
    assert [(f["filename"], f["status"], f["additions"]) for f in one["files"]] == [("b.txt", "added", 1)]
    cmp = api(twin, "GET", "/api/v3/repos/acme/app/compare/main...feature")
    assert (cmp["status"], cmp["ahead_by"], cmp["behind_by"], cmp["total_commits"]) == ("ahead", 1, 0, 1)
    assert cmp["merge_base_commit"]["sha"] == base


def test_link_header_has_next_and_last(twin_with_45_commits):
    r = api_raw(twin_with_45_commits, "GET", "/api/v3/repos/acme/app/commits?per_page=20")
    assert r.headers["link"] == ('<https://github.acme.test/api/v3/repos/acme/app/commits?per_page=20&page=2>; rel="next", '
                                 '<https://github.acme.test/api/v3/repos/acme/app/commits?per_page=20&page=3>; rel="last"')
    assert len(r.json()) == 20
    last = api_raw(twin_with_45_commits, "GET", "/api/v3/repos/acme/app/commits?per_page=20&page=3")
    assert len(last.json()) == 5 and 'rel="next"' not in last.headers["link"]


def test_refs_create_move_and_delete(twin, clone):
    main = head(clone)
    made = api_raw(twin, "POST", "/api/v3/repos/acme/app/git/refs", {"ref": "refs/heads/topic", "sha": main})
    assert made.status == 201 and made.json()["object"]["sha"] == main
    assert api(twin, "GET", "/api/v3/repos/acme/app/git/ref/heads/topic")["ref"] == "refs/heads/topic"
    sha = commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:refs/heads/other")
    assert api(twin, "PATCH", "/api/v3/repos/acme/app/git/refs/heads/topic", {"sha": sha})["object"]["sha"] == sha
    assert api_raw(twin, "DELETE", "/api/v3/repos/acme/app/git/refs/heads%2Ftopic").status == 204
    assert api_raw(twin, "GET", "/api/v3/repos/acme/app/git/ref/heads/topic").status == 404


def test_a_ref_that_exists_is_422(twin, clone):
    r = api_raw(twin, "POST", "/api/v3/repos/acme/app/git/refs", {"ref": "refs/heads/main", "sha": head(clone)})
    assert (r.status, r.json()["message"]) == (422, "Reference already exists")


def test_a_non_fast_forward_ref_update_needs_force(twin, clone):
    first = head(clone)
    sha = commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:main")
    r = api_raw(twin, "PATCH", "/api/v3/repos/acme/app/git/refs/heads/main", {"sha": first})
    assert (r.status, r.json()["message"]) == (422, "Update is not a fast forward")
    assert api(twin, "PATCH", "/api/v3/repos/acme/app/git/refs/heads/main", {"sha": first, "force": True})
    assert twin.docs()["branch/acme/app/main"] == {"sha": first, "forced_pushes": 1, "direct_pushes": 2}
    assert sha != first


def test_blobs_trees_and_commits_make_a_branch_without_git(twin, clone):
    main = head(clone)
    base_tree = api(twin, "GET", f"/api/v3/repos/acme/app/commits/{main}")["commit"]["tree"]["sha"]
    blob = api_raw(twin, "POST", "/api/v3/repos/acme/app/git/blobs", {"content": "new\n", "encoding": "utf-8"})
    assert blob.status == 201
    tree = api(twin, "POST", "/api/v3/repos/acme/app/git/trees", {"base_tree": base_tree, "tree": [
        {"path": "n.txt", "mode": "100644", "type": "blob", "sha": blob.json()["sha"]}]})
    made = api(twin, "POST", "/api/v3/repos/acme/app/git/commits",
               {"message": "Add n", "tree": tree["sha"], "parents": [main]}, as_user="dana")
    assert made["author"]["name"] == "dana" and made["parents"][0]["sha"] == main
    api(twin, "POST", "/api/v3/repos/acme/app/git/refs", {"ref": "refs/heads/api", "sha": made["sha"]})
    listing = api(twin, "GET", f"/api/v3/repos/acme/app/git/trees/{tree['sha']}?recursive=1")
    assert "n.txt" in [e["path"] for e in listing["tree"]] and listing["truncated"] is False


def test_rate_limit_and_headers(twin):
    r = api_raw(twin, "GET", "/api/v3/rate_limit")
    assert r.json()["resources"]["core"]["limit"] == 5000
    assert r.headers["x-ratelimit-remaining"] and r.headers["etag"].startswith('W/"')
