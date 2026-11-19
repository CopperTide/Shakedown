"""Live: build the base and app images with images/build.sh, then run each repository's CI test step in the base
image the way the GitHub twin's runner does (no network, 384 MiB, this user, HOME=/tmp, the checkout mounted)."""
import os
import shutil
import subprocess

import pytest
from conftest import REPOS
from support import load_yaml

pytestmark = pytest.mark.live
WORKSPACE = "/github/workspace"


def docker(*argv, timeout=900) -> subprocess.CompletedProcess:
    return subprocess.run(["docker", *argv], capture_output=True, text=True, timeout=timeout)


@pytest.fixture(scope="module")
def built(sf_dir):
    if shutil.which("docker") is None:
        pytest.skip("docker is not installed")
    done = subprocess.run([str(sf_dir / "images" / "build.sh"), "all"], capture_output=True, text=True, timeout=1800)
    assert done.returncode == 0, done.stdout[-3000:] + done.stderr[-3000:]
    return {i["id"]: i["image"] for i in load_yaml(sf_dir / "images.yaml")["images"]}


def ci_test_command(sf_dir, repo) -> str:
    wf = load_yaml(sf_dir / "repos" / repo / ".github" / "workflows" / "ci.yml")
    return wf["jobs"]["test"]["steps"][-1]["run"]


@pytest.mark.parametrize("repo", REPOS)
def test_live_each_repos_ci_tests_pass_in_the_base_image(built, sf_dir, tmp_path, repo):
    checkout = tmp_path / repo
    shutil.copytree(sf_dir / "repos" / repo, checkout, ignore=shutil.ignore_patterns("__pycache__", ".pytest_cache"))
    argv = ["run", "--rm", "--pull", "never", "--network", "none", "--memory", "384m", "--memory-swap", "384m",
            "--user", f"{os.getuid()}:{os.getgid()}", "-e", "HOME=/tmp", "-e", "CI=true",
            "-v", f"{checkout}:{WORKSPACE}", "-w", WORKSPACE, built["base"], "sh", "-euc",
            ci_test_command(sf_dir, repo)]
    done = docker(*argv)
    flaky = "test_a_hold_lasts_while_the_customer_pays"
    if done.returncode != 0 and repo == "inventory" and f"FAILED tests/test_reservations.py::{flaky}" in done.stdout \
            and done.stdout.count("FAILED") == 1:
        pytest.xfail("inventory's hold test is flaky by design (about one run in four)")
    assert done.returncode == 0, done.stdout[-3000:] + done.stderr[-3000:]


@pytest.mark.parametrize("repo", REPOS)
def test_live_each_app_image_reports_its_version_deps(built, repo):
    image = built[repo]
    code = ("import json, pathlib; from platform_lib.version import version_doc; "
            "print(json.dumps(version_doc(pathlib.Path('/app/requirements.txt'))))")
    done = docker("run", "--rm", "--pull", "never", "--network", "none", "-e", f"APP_IMAGE={image}", image,
                  "python", "-c", code)
    assert done.returncode == 0, done.stderr
    doc = __import__("json").loads(done.stdout)
    assert doc["sha"] == image.rsplit(":", 1)[1] and doc["deps"]["platform_lib_major"] == 1
    assert (doc["deps"].get("pricing_lib_major") == 1) == (repo == "inventory")
