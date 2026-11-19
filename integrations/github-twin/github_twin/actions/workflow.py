"""The workflow subset this world's Actions runs: `name`, `on: push|pull_request` (+ `branches`),
`env`, and jobs with `name`, `runs-on`, `container.image`, `needs`, `if`, `env` and steps of `run`
or the three actions it knows. Anything else is a `WorkflowError` naming where it is."""
import fnmatch
import json
import re
from dataclasses import asdict, dataclass, field

import yaml

ACTIONS = {"actions/checkout@v4": (), "shakedown/build@v1": ("image",), "shakedown/deploy@v1": ("service", "image")}
OPTIONAL_WITH = {"shakedown/build@v1": ("context",)}
EVENTS = ("push", "pull_request")
TOP, JOB, STEP = {"name", "on", "env", "jobs"}, {"name", "runs-on", "container", "needs", "if", "env", "steps"}, \
    {"name", "run", "uses", "with", "env"}
CONDITION = re.compile(r"^(github\.ref|github\.event_name)\s*(==|!=)\s*'([^']*)'$")


class WorkflowError(Exception):
    """Why a workflow cannot run: `<where>: <what>`."""


def unsupported(where: str, what: str) -> WorkflowError:
    return WorkflowError(f"{where}: {what} is not supported by this world's Actions")


@dataclass(frozen=True)
class Step:
    name: str
    run: str | None = None
    uses: str | None = None
    with_: dict = field(default_factory=dict)
    env: dict = field(default_factory=dict)


@dataclass(frozen=True)
class Job:
    key: str
    name: str
    runs_on: str | None
    image: str | None
    needs: tuple
    condition: tuple | None
    env: dict
    steps: tuple

    def runs_if(self, github: dict) -> bool:
        """Whether the job's `if` holds for `github` (`{"github.ref": …, "github.event_name": …}`)."""
        if self.condition is None:
            return True
        name, op, value = self.condition
        return (github.get(name) == value) == (op == "==")


@dataclass(frozen=True)
class Workflow:
    name: str | None
    on: dict
    env: dict
    jobs: tuple

    def triggered(self, event: str, branch: str) -> bool:
        """Whether `event` on `branch` (pushed, or a pull request's base) starts this workflow."""
        if event not in self.on:
            return False
        patterns = self.on[event]
        return patterns is None or any(fnmatch.fnmatchcase(branch, p) for p in patterns)

    def to_json(self) -> str:
        return json.dumps(asdict(self))

    @staticmethod
    def from_json(text: str) -> "Workflow":
        d = json.loads(text)
        jobs = tuple(Job(**{**j, "needs": tuple(j["needs"]), "condition": tuple(j["condition"]) if j["condition"] else None,
                            "steps": tuple(Step(**s) for s in j["steps"])}) for j in d["jobs"])
        return Workflow(d["name"], {k: tuple(v) if v is not None else None for k, v in d["on"].items()}, d["env"], jobs)


def _only(where: str, node: dict, allowed: set) -> None:
    for key in node:
        if key not in allowed:
            raise unsupported(f"{where}.{key}" if where else str(key), str(key))


def _map(where: str, node) -> dict:
    if node is None:
        return {}
    if not isinstance(node, dict):
        raise WorkflowError(f"{where}: a mapping is needed")
    return node


def _text(where: str, value) -> str:
    text = "" if value is None else str(value)
    if "${{" in text:
        raise WorkflowError(f"{where}: ${{{{ }}}} expressions are not supported by this world's Actions")
    return text


def _env(where: str, node) -> dict:
    return {str(k): _text(f"{where}.{k}", v) for k, v in _map(where, node).items()}


def _branches(where: str, node) -> tuple | None:
    _only(where, _map(where, node), {"branches"})
    if not node or node.get("branches") is None:
        return None
    b = node["branches"]
    return tuple(str(x) for x in (b if isinstance(b, list) else [b]))


def _on(node) -> dict:
    if isinstance(node, str):
        node = [node]
    if isinstance(node, list):
        node = {e: None for e in node}
    out = {}
    for event, spec in _map("on", node).items():
        if event not in EVENTS:
            raise unsupported(f"on.{event}", str(event))
        out[event] = _branches(f"on.{event}", spec)
    return out


def _with(where: str, uses: str, node) -> dict:
    given = {str(k): _text(f"{where}.{k}", v) for k, v in _map(where, node).items()}
    for key in given:
        if key not in ACTIONS[uses] + OPTIONAL_WITH.get(uses, ()):
            raise unsupported(f"{where}.{key}", key)
    for key in ACTIONS[uses]:
        if not given.get(key):
            raise WorkflowError(f"{where}: {uses} needs {key}")
    return given


def _step(where: str, node) -> Step:
    node = _map(where, node)
    _only(where, node, STEP)
    run, uses = node.get("run"), node.get("uses")
    if (run is None) == (uses is None):
        raise WorkflowError(f"{where}: a step has either run or uses")
    if uses is not None and uses not in ACTIONS:
        raise unsupported(f"{where}.uses", str(uses))
    run = _text(f"{where}.run", run) if run is not None else None
    name = _text(f"{where}.name", node.get("name")) or f"Run {(run or uses).strip().splitlines()[0]}"
    return Step(name, run, uses, _with(f"{where}.with", uses, node.get("with")) if uses else {},
                _env(f"{where}.env", node.get("env")))


def _condition(where: str, value) -> tuple | None:
    if value is None:
        return None
    text = str(value).strip()
    inner = text[3:-2].strip() if text.startswith("${{") and text.endswith("}}") else text
    m = CONDITION.match(inner)
    if m is None:
        raise unsupported(where, text)
    return m.groups()


def _image(where: str, node) -> str | None:
    if node is None or isinstance(node, str):
        return node
    _only(where, _map(where, node), {"image"})
    return _text(f"{where}.image", node.get("image")) or None


def _job(key: str, node) -> Job:
    where = f"jobs.{key}"
    node = _map(where, node)
    _only(where, node, JOB)
    steps = node.get("steps")
    if not isinstance(steps, list) or not steps:
        raise WorkflowError(f"{where}.steps: a job needs steps")
    needs = node.get("needs") or []
    return Job(key, _text(f"{where}.name", node.get("name")) or key, node.get("runs-on"),
               _image(f"{where}.container", node.get("container")),
               tuple(needs if isinstance(needs, list) else [needs]), _condition(f"{where}.if", node.get("if")),
               _env(f"{where}.env", node.get("env")),
               tuple(_step(f"{where}.steps[{i}]", s) for i, s in enumerate(steps)))


def _check_needs(jobs: tuple) -> None:
    keys, done = {j.key for j in jobs}, set()
    for j in jobs:
        for n in j.needs:
            if n not in keys:
                raise WorkflowError(f"jobs.{j.key}.needs: no job {n}")
    todo = list(jobs)
    while todo:
        ready = [j for j in todo if set(j.needs) <= done]
        if not ready:
            raise WorkflowError(f"jobs.{todo[0].key}.needs: the jobs need each other")
        done |= {j.key for j in ready}
        todo = [j for j in todo if j not in ready]


def _workflow(doc) -> Workflow:
    doc = _map("", doc)
    if True in doc:  # YAML 1.1 reads the key `on` as true
        doc = {("on" if k is True else k): v for k, v in doc.items()}
    _only("", doc, TOP)
    if not doc.get("jobs"):
        raise WorkflowError("jobs: a workflow needs jobs")
    jobs = tuple(_job(str(k), v) for k, v in _map("jobs", doc["jobs"]).items())
    _check_needs(jobs)
    name = doc.get("name")
    return Workflow(str(name) if name is not None else None, _on(doc.get("on")), _env("env", doc.get("env")), jobs)


def parse(text: str) -> Workflow | WorkflowError:
    """The workflow `text` describes, or why this world's Actions cannot run it."""
    try:
        doc = yaml.safe_load(text)
    except yaml.YAMLError as e:
        return WorkflowError(f"invalid YAML: {e}".replace("\n", " "))
    try:
        return _workflow(doc)
    except WorkflowError as e:
        return e
