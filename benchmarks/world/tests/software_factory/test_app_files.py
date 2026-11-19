"""The sample app's repositories, read as files: contracts, pipelines, images and pins."""
import json
import re

import jsonschema
import pytest
from conftest import REPOS
from sf_support import load_yaml

# The workflow subset the GitHub twin's Actions runs (Task 5): keys per level and the actions it knows.
TOP, JOB, STEP = {"name", "on", "env", "jobs"}, {"name", "runs-on", "container", "needs", "if", "env", "steps"}, \
    {"name", "run", "uses", "with", "env"}
ACTIONS = {"actions/checkout@v4", "shakedown/build@v1", "shakedown/deploy@v1"}
CONDITION = re.compile(r"^(github\.ref|github\.event_name)\s*(==|!=)\s*'[^']*'$")
PIN = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)(?:\[[^\]]*\])?==([^\s;]+)")
BUILT_HERE = {"platform-lib": "1.0.0", "pricing-lib": "1.4.0"}


def workflow(sf_dir, repo) -> dict:
    doc = load_yaml(sf_dir / "repos" / repo / ".github" / "workflows" / "ci.yml")
    return {("on" if k is True else k): v for k, v in doc.items()}


def pins(path) -> dict:
    lines = (line.split("#", 1)[0].strip() for line in path.read_text(encoding="utf-8").splitlines())
    return {m.group(1).lower(): m.group(2) for m in map(PIN.match, lines) if m}


@pytest.mark.parametrize("repo", REPOS)
def test_contracts_meet_the_schema_and_name_real_providers(sf_dir, repo):
    schema = json.loads((sf_dir / "contracts.schema.json").read_text(encoding="utf-8"))
    for path in (sf_dir / "repos" / repo / "contracts").glob("*.json"):
        contract = json.loads(path.read_text(encoding="utf-8"))
        jsonschema.validate(contract, schema)
        assert contract["consumer"] == repo and contract["provider"] == path.stem and path.stem in REPOS


def test_storefront_reads_the_order_total_in_units(sf_dir):
    contract = json.loads((sf_dir / "repos/storefront/contracts/orders.json").read_text(encoding="utf-8"))
    assert {"method": "GET", "path": "/orders/1"}.items() <= contract["calls"][0].items()
    assert contract["calls"][0]["fields"]["total"] == "number"


@pytest.mark.parametrize("repo", REPOS)
def test_the_pipeline_stays_inside_the_actions_subset(sf_dir, repo):
    wf = workflow(sf_dir, repo)
    assert set(wf) <= TOP and "${{" not in json.dumps(wf)
    for job in wf["jobs"].values():
        assert set(job) <= JOB and CONDITION.match(job.get("if", "github.ref == 'x'"))
        for step in job["steps"]:
            assert set(step) <= STEP and ("run" in step) != ("uses" in step)
            assert step.get("uses", "actions/checkout@v4") in ACTIONS


@pytest.mark.parametrize("repo", REPOS)
def test_tests_run_on_pull_requests_and_main_deploys_staging_then_production(sf_dir, repo):
    wf = workflow(sf_dir, repo)
    assert set(wf["on"]) == {"push", "pull_request"}
    test, deploy = wf["jobs"]["test"], wf["jobs"]["deploy"]
    assert "if" not in test and "python -m pytest" in test["steps"][-1]["run"]
    assert deploy["needs"] == "test" and deploy["if"] == "github.ref == 'refs/heads/main'"
    acts = [(s["uses"], s.get("with", {})) for s in deploy["steps"]]
    assert acts == [("actions/checkout@v4", {}), ("shakedown/build@v1", {"image": repo}),
                    ("shakedown/deploy@v1", {"service": f"staging-{repo}", "image": repo}),
                    ("shakedown/deploy@v1", {"service": f"prod-{repo}", "image": repo})]


@pytest.mark.parametrize("repo", REPOS)
def test_every_pin_is_in_the_wheelhouse(sf_dir, repo):
    house = {**pins(sf_dir / "images/wheelhouse/requirements.txt"), **BUILT_HERE}
    for file in ("requirements.txt", "requirements-dev.txt"):
        for name, version in pins(sf_dir / "repos" / repo / file).items():
            assert house.get(name) == version, (repo, file, name)


@pytest.mark.parametrize("repo", REPOS)
def test_each_image_builds_from_the_base_image(sf_dir, repo):
    base = next(i["image"] for i in load_yaml(sf_dir / "images.yaml")["images"] if i["id"] == "base")
    first = (sf_dir / "repos" / repo / "Dockerfile").read_text(encoding="utf-8").splitlines()[0]
    assert first == f"FROM {base}"


def test_the_wheelhouse_holds_both_pricing_lib_releases(sf_dir):
    assert {p.name for p in (sf_dir / "images/wheelhouse").glob("pricing-lib-*")} == {
        "pricing-lib-1.4.0", "pricing-lib-2.0.0"}
    assert pins(sf_dir / "repos/inventory/requirements.txt")["pricing-lib"] == "1.4.0"


def test_the_base_image_builds_from_the_pinned_python(sf_dir):
    python = next(i["image"] for i in load_yaml(sf_dir / "images.yaml")["images"] if i["id"] == "python")
    lines = (sf_dir / "images/base/Dockerfile").read_text(encoding="utf-8").splitlines()
    assert next(line for line in lines if line.startswith("FROM ")) == f"FROM {python}"
