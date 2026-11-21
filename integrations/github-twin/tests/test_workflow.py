from github_twin.actions.workflow import Workflow, WorkflowError, parse

CI = """
name: CI
on:
  push:
    branches: [main, "release/*"]
  pull_request:
env:
  CI: "true"
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Tests
        run: python -m pytest -q
  deploy:
    name: Deploy
    needs: test
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    container:
      image: base:1
    env:
      STAGE: prod
    steps:
      - uses: shakedown/build@v1
        with: {image: orders, context: services/orders}
      - uses: shakedown/deploy@v1
        with: {service: prod-orders, image: orders}
"""

WF_WITH_MATRIX = """
name: CI
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix: {py: ["3.11", "3.12"]}
    steps:
      - run: pytest -q
"""


def wf(text: str) -> Workflow:
    out = parse(text)
    assert isinstance(out, Workflow), str(out)
    return out


def error(text: str) -> str:
    out = parse(text)
    assert isinstance(out, WorkflowError), out
    return str(out)


def test_the_subset_parses():
    w = wf(CI)
    assert w.name == "CI" and w.env == {"CI": "true"}
    test, deploy = w.jobs
    assert (test.key, test.name, test.runs_on, test.image, test.needs) == ("test", "test", "ubuntu-latest", None, ())
    assert [(s.name, s.run, s.uses) for s in test.steps] == [
        ("Run actions/checkout@v4", None, "actions/checkout@v4"), ("Tests", "python -m pytest -q", None)]
    assert (deploy.name, deploy.image, deploy.needs, deploy.env) == ("Deploy", "base:1", ("test",), {"STAGE": "prod"})
    assert deploy.steps[0].with_ == {"image": "orders", "context": "services/orders"}


def test_on_selects_events_and_branches():
    w = wf(CI)
    assert w.triggered("push", "main") and w.triggered("push", "release/1.2")
    assert not w.triggered("push", "feature") and w.triggered("pull_request", "anything")
    assert wf("on: [pull_request]\njobs: {t: {runs-on: x, steps: [{run: 'true'}]}}").triggered("pull_request", "main")
    assert not wf("on: pull_request\njobs: {t: {runs-on: x, steps: [{run: 'true'}]}}").triggered("push", "main")


def test_a_job_condition_reads_the_ref_and_event():
    deploy = wf(CI).jobs[1]
    assert deploy.runs_if({"github.ref": "refs/heads/main", "github.event_name": "push"})
    assert not deploy.runs_if({"github.ref": "refs/heads/f", "github.event_name": "push"})
    job = wf("on: push\njobs: {t: {runs-on: x, if: \"${{ github.event_name != 'push' }}\", steps: [{run: 'true'}]}}").jobs[0]
    assert not job.runs_if({"github.ref": "refs/heads/main", "github.event_name": "push"})


def test_a_workflow_survives_its_json():
    w = wf(CI)
    assert Workflow.from_json(w.to_json()) == w


def test_an_unsupported_key_fails_the_run_at_startup():
    assert str(parse(WF_WITH_MATRIX)) == "jobs.test.strategy: strategy is not supported by this world's Actions"


def test_what_the_subset_refuses():
    base = "on: push\njobs:\n  t:\n    runs-on: x\n    steps:\n      - "
    assert error(base + "uses: actions/setup-python@v5") == (
        "jobs.t.steps[0].uses: actions/setup-python@v5 is not supported by this world's Actions")
    assert error(base + "run: echo ${{ github.sha }}") == (
        "jobs.t.steps[0].run: ${{ }} expressions are not supported by this world's Actions")
    assert error("on: schedule\njobs: {t: {runs-on: x, steps: [{run: 'true'}]}}") == (
        "on.schedule: schedule is not supported by this world's Actions")
    assert error("on: push\njobs: {t: {runs-on: x, if: \"github.actor == 'x'\", steps: [{run: 'true'}]}}") == (
        "jobs.t.if: github.actor == 'x' is not supported by this world's Actions")
    assert error("on: push\njobs: {t: {runs-on: x, needs: u, steps: [{run: 'true'}]}}") == "jobs.t.needs: no job u"
    assert error("on: push\njobs: {t: {runs-on: x, steps: [{uses: shakedown/deploy@v1, with: {service: s}}]}}") == (
        "jobs.t.steps[0].with: shakedown/deploy@v1 needs image")
    assert error("on: push\njobs: {t: {runs-on: x, steps: []}}") == "jobs.t.steps: a job needs steps"
    assert error("on: push\njobs: [") .startswith("invalid YAML: ")
