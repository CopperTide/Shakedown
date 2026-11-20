import os

from conftest import START_MS, Twin, commit, git, head, push, raw_get


def test_clone_push_and_fetch_over_smart_http(twin, tmp_path):
    git(tmp_path, "clone", "-q", twin.git_url("acme", "app"), "app", creds=twin.token)
    assert sorted(p.name for p in (tmp_path / "app").iterdir() if p.name != ".git") == ["README.md", "app.py", "test_app.py"]
    sha = commit(tmp_path / "app", "b.txt", "two")
    push(tmp_path / "app", twin.token, "HEAD:refs/heads/feature")
    assert twin.docs()["branch/acme/app/feature"] == {"sha": sha, "forced_pushes": 0, "direct_pushes": 1}
    git(tmp_path, "clone", "-q", twin.git_url("acme", "app"), "again", creds=twin.token)
    git(tmp_path / "again", "fetch", "-q", "origin", "feature", creds=twin.token)
    assert git(tmp_path / "again", "rev-parse", "FETCH_HEAD").stdout.strip() == sha


def test_the_seed_commit_is_dated_in_world_time_by_the_default_user(clone):
    name, at = git(clone, "log", "-1", "--format=%an|%at").stdout.strip().split("|")
    assert name == "coder" and START_MS // 1000 < int(at) <= START_MS // 1000 + 5   # the test clock ticks 1 s a read


def test_the_repo_is_served_without_the_git_suffix_too(twin, tmp_path):
    git(tmp_path, "clone", "-q", f"{twin.up}/acme/app", "app", creds=twin.token)
    assert (tmp_path / "app" / "app.py").is_file()


def test_protocol_v2_clone(twin, tmp_path):
    git(tmp_path, "-c", "protocol.version=2", "clone", "-q", twin.git_url("acme", "app"), "app", creds=twin.token)
    r = raw_get(twin.up, "/acme/app.git/info/refs?service=git-upload-pack",
                headers={"Authorization": f"Bearer {twin.token}", "Git-Protocol": "version=2"})
    assert r.status == 200 and r.body.startswith(b"000eversion 2\n")
    assert r.headers["content-type"] == "application/x-git-upload-pack-advertisement"


def test_anonymous_git_gets_a_basic_challenge(twin):
    r = raw_get(twin.up, "/acme/app.git/info/refs?service=git-upload-pack")
    assert (r.status, r.headers["www-authenticate"]) == (401, 'Basic realm="GitHub"')


def test_a_wrong_password_is_challenged_again(twin, tmp_path):
    r = git(tmp_path, "clone", "-q", twin.git_url("acme", "app"), "app", creds="wrong", check=False)
    assert r.returncode != 0 and "Authentication failed" in r.stderr


def test_a_missing_repo_is_404(twin):
    r = raw_get(twin.up, "/acme/nope.git/info/refs?service=git-upload-pack",
                headers={"Authorization": f"token {twin.token}"})
    assert r.status == 404


def test_upload_pack_posts_are_reads(twin):
    route = twin.part.http_routes("i")["routes"][0]
    assert route["reads"] == ["POST */git-upload-pack"]
    assert route["env"]["GITHUB_API_URL"] == "https://github.acme.test/api/v3"
    assert route["env"]["GH_ENTERPRISE_TOKEN"] == twin.token
    assert route["env"]["GIT_CONFIG_KEY_0"] == "credential.https://github.acme.test.helper"


def test_a_forced_push_is_counted(twin, clone):
    commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:refs/heads/feature")
    git(clone, "commit", "-q", "--amend", "-m", "Rewritten")
    push(clone, twin.token, "--force", "HEAD:refs/heads/feature")
    assert twin.docs()["branch/acme/app/feature"] == {"sha": head(clone), "forced_pushes": 1, "direct_pushes": 2}


def test_a_save_links_git_objects(twin, clone, saves_root):
    commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:main")
    twin.part.save("i", "s1", str(saves_root / "github-twin" / "s1"))
    twin.part.save("i", "s2", str(saves_root / "github-twin" / "s2"))
    objs = [p for p in (saves_root / "github-twin" / "s1" / "files").rglob("*")
            if p.is_file() and "objects" in p.parts and "info" not in p.parts]
    assert objs
    for p in objs:
        other = saves_root / "github-twin" / "s2" / p.relative_to(saves_root / "github-twin" / "s1")
        assert os.stat(p).st_ino == os.stat(other).st_ino
    refs = saves_root / "github-twin" / "s1" / "files" / "acme" / "app.git" / "HEAD"
    assert os.stat(refs).st_ino != os.stat(saves_root / "github-twin" / "s2" / refs.relative_to(saves_root / "github-twin" / "s1")).st_ino


def test_a_restored_instance_serves_the_saved_repos(twin, clone, saves_root, tmp_path):
    sha = commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:main")
    twin.part.save("i", "s1", str(saves_root / "github-twin" / "s1"))
    commit(clone, "c.txt", "three")
    push(clone, twin.token, "HEAD:main")
    twin.part.start("j", twin.part.ctx("i").config, 1, 0, {"save": "s1", "mode": "exact"})
    try:
        other = Twin(twin.part, "j")
        assert other.docs()["branch/acme/app/main"]["sha"] == sha
        git(tmp_path, "clone", "-q", other.git_url("acme", "app"), "restored", creds=other.token)
        assert head(tmp_path / "restored") == sha
    finally:
        twin.part.stop("j")
