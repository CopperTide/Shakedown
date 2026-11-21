"""The REST answers `gh run list/view/watch` read (research §2.5), on a fake Docker."""
import io
import zipfile

from conftest import HOST, api, api_raw, drain, push_commit

R = "/api/v3/repos/acme/app"
RUN_FIELDS = {"id", "name", "display_title", "status", "conclusion", "event", "workflow_id", "run_number",
              "run_attempt", "head_branch", "head_sha", "head_commit", "html_url", "jobs_url", "created_at",
              "updated_at", "run_started_at", "head_repository"}


def _one_run(t):
    push_commit(t, "main", "b.py", "B = 2\n", "Add b\n\nMore words.")
    [run] = drain(t)
    return run


def test_gh_run_list_shape(twin_fake_docker):
    run = _one_run(twin_fake_docker)
    listing = api(twin_fake_docker, "GET", f"{R}/actions/runs?per_page=20&exclude_pull_requests=true")
    assert set(listing) == {"total_count", "workflow_runs"} and listing["total_count"] == 1
    assert RUN_FIELDS <= set(run) and run["display_title"] == "Add b"
    assert run["head_commit"]["message"] == "Add b\n\nMore words."
    wfs = api(twin_fake_docker, "GET", f"{R}/actions/workflows?per_page=100")
    assert [(w["id"], w["name"], w["path"], w["state"]) for w in wfs["workflows"]] == [
        (run["workflow_id"], "CI", ".github/workflows/ci.yml", "active")]
    by_file = api(twin_fake_docker, "GET", f"{R}/actions/workflows/ci.yml/runs")
    assert [r["id"] for r in by_file["workflow_runs"]] == [run["id"]]


def test_runs_filter_like_github(twin_fake_docker):
    t = twin_fake_docker
    run = _one_run(t)
    def ids(q):
        return [r["id"] for r in api(t, "GET", f"{R}/actions/runs?{q}")["workflow_runs"]]
    assert ids("branch=main") == ids("event=push") == ids("status=success") == ids("status=completed") == [run["id"]]
    assert ids("branch=f") == ids("event=pull_request") == ids("status=failure") == ids("actor=nobody") == []
    assert ids(f"head_sha={run['head_sha']}") == [run["id"]]


def test_jobs_url_is_the_twins(twin_fake_docker):   # Review Focus 1
    run = _one_run(twin_fake_docker)
    base = f"https://{HOST}{R}/actions/runs/{run['id']}"
    assert (run["jobs_url"], run["logs_url"], run["url"]) == (f"{base}/jobs", f"{base}/logs", base)
    jobs = api(twin_fake_docker, "GET", run["jobs_url"].removeprefix(f"https://{HOST}"))
    assert set(jobs) == {"total_count", "jobs"}
    [job] = jobs["jobs"]
    assert (job["run_id"], job["name"], job["status"], job["conclusion"]) == (run["id"], "test", "completed", "success")
    assert [(s["number"], s["name"]) for s in job["steps"]] == [(1, "Run actions/checkout@v4"), (2, "Tests")]
    assert api(twin_fake_docker, "GET", f"{R}/check-runs/{job['id']}/annotations") == []


def test_run_logs_are_a_zip_of_each_jobs_steps(twin_fake_docker):
    t = twin_fake_docker
    t.docker.answer("unittest", 0, "Ran 1 test\nOK\n")
    run = _one_run(t)
    r = api_raw(t, "GET", f"{R}/actions/runs/{run['id']}/logs")
    assert r.status == 302 and r.headers["location"].startswith(f"https://{HOST}/_logs/runs/{run['id']}")
    z = api_raw(t, "GET", r.headers["location"].removeprefix(f"https://{HOST}"), token="wrong")
    assert z.status == 200 and z.headers["content-type"] == "application/zip"
    files = zipfile.ZipFile(io.BytesIO(z.body))
    assert sorted(files.namelist()) == ["0_test.txt", "test/1_Run actions_checkout@v4.txt", "test/2_Tests.txt"]
    assert "Ran 1 test" in files.read("test/2_Tests.txt").decode()
    tampered = r.headers["location"].removeprefix(f"https://{HOST}").replace("sig=", "sig=0")
    assert api_raw(t, "GET", tampered).status == 404


def test_a_run_without_its_artifacts(twin_fake_docker):
    run = _one_run(twin_fake_docker)
    assert api(twin_fake_docker, "GET", f"{R}/actions/runs/{run['id']}/artifacts") == {"total_count": 0, "artifacts": []}
    assert api_raw(twin_fake_docker, "GET", f"{R}/actions/runs/999").status == 404
