"""Owner ruling (2026-11-19): a read-only agent's requests reach the twin marked `X-Arena-Readonly: 1`;
the twin serves its reads and refuses its writes itself."""
from conftest import api_raw, commit, raw

R = "/api/v3/repos/acme/app"
RO = {"X-Arena-Readonly": "1"}


def test_the_twin_says_it_honours_read_only(twin):
    assert twin.part.http_routes("i")["routes"][0]["honours_readonly"] is True


def test_a_read_only_agent_reads(twin):
    r = api_raw(twin, "GET", R, as_user="auditor", headers=RO)
    assert r.status == 200 and r.json()["full_name"] == "acme/app"


def test_a_read_only_agent_cannot_write(twin):
    r = api_raw(twin, "POST", f"{R}/issues", {"title": "Sneaky"}, as_user="auditor", headers=RO)
    assert (r.status, r.json()["message"]) == (403, "Resource not accessible by integration")
    assert r.json()["documentation_url"].startswith("https://docs.github.com/")
    assert r.headers["x-arena-effect"] == "read"
    assert api_raw(twin, "GET", f"{R}/issues").json() == []
    assert twin.part.advance("i", 100)["changed"] is False


def test_a_read_only_agent_clones_but_cannot_push(twin, clone):
    # The clone's upload-pack POST carried no read-only mark; run its git requests by hand with one.
    auth = {"Authorization": f"token {twin.token}", **RO}
    refs = raw(twin.up, "GET", "/acme/app.git/info/refs?service=git-upload-pack", headers=auth)
    assert refs.status == 200
    body = b"0000"   # a flush: upload-pack answers an empty negotiation with nothing, and does not fail
    pack = raw(twin.up, "POST", "/acme/app.git/git-upload-pack", body,
               {**auth, "Content-Type": "application/x-git-upload-pack-request"})
    assert pack.status == 200
    commit(clone, "b.txt", "two")
    push = raw(twin.up, "POST", "/acme/app.git/git-receive-pack", b"0000",
               {**auth, "Content-Type": "application/x-git-receive-pack-request"})
    assert (push.status, push.headers["x-arena-effect"]) == (403, "read")
