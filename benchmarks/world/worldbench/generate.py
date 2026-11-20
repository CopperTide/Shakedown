"""A world from a template, the user's choices and a seed: the same bytes every time (format
`shakedown-world/1`)."""
import copy
import json
from dataclasses import dataclass, field, replace
from pathlib import Path

from . import select
from .choices import Choices, problem
from .errors import TemplateError
from .events import apply_events, apply_systems
from .expand import Context, expand
from .people import apply_edits, generate_people
from .slug import key_of, slug_of

FORMAT = "shakedown-world/1"
NO_TASK = problem("tasks", "every task needs a system that is not chosen")


@dataclass
class Result:
    """The world (None when there are problems), the folders it copies (`(source, "files/<path>")`), what was
    dropped and why, the problems, and the people drawn (also when other problems left no world)."""
    world: dict | None
    files: list[tuple[Path, str]] = field(default_factory=list)
    dropped: list = field(default_factory=list)
    problems: list = field(default_factory=list)
    people: list = field(default_factory=list)


def world_json(world: dict) -> str:
    return json.dumps(world, sort_keys=True, indent=1, ensure_ascii=False) + "\n"


def generate(t, raw_choices: dict) -> Result:
    """The world of `raw_choices`; with problems, no world but the people still drawn (the choices that don't fit
    are left at their defaults), so the builder's People step keeps showing them."""
    c, problems = Choices.parse(t, raw_choices)
    kept = select.keep(t, c.systems)
    problems += [] if kept.tasks else [NO_TASK]
    problems += _added_roles(c, kept.roles)
    try:
        people, edit_problems = apply_edits(generate_people(t, c, kept.roles), c)
    except TemplateError as e:
        return Result(None, dropped=kept.dropped, problems=problems + [problem(e.path, e.message)])
    if problems or edit_problems:
        return Result(None, dropped=kept.dropped, problems=problems + edit_problems, people=people)
    try:
        world, files = _build(t, c, kept, people)
    except TemplateError as e:
        return Result(None, dropped=kept.dropped, problems=[problem(e.path, e.message)], people=people)
    return Result(world, files, kept.dropped, people=world["people"])


def _added_roles(c: Choices, roles: list) -> list:
    ids = {r["id"] for r in roles}
    return [problem(f"people.added[{i}].role", f"no role {a['role']} in this world")
            for i, a in enumerate(c.added) if a["role"] not in ids]


def company_of(t, name: str) -> dict:
    """The company placeholders read: name, slug, key, domain (and the template's clock start)."""
    slug = slug_of(name)
    company = {"name": name, "slug": slug, "key": key_of(name), "domain": f"{slug}.test"}
    if "clock_start" in t.data["company"]:
        company["clock_start"] = t.data["company"]["clock_start"]
    return company


def _build(t, c: Choices, kept: select.Kept, people: list) -> tuple[dict, list]:
    company = company_of(t, c.company)
    ctx = Context(company=company, size=c.size, options=c.options, people=people, params={}, seed=c.seed)
    files: list = []

    def fill(value, path: str):
        return expand(value, ctx, t.dir, files, path)

    world = {"format": FORMAT, "name": company["slug"], "title": c.company, "seed": c.seed,
             "template": {"id": t.id, "name": t.name, "version": t.version}, "choices": c.to_json(),
             "company": company, "people": people, "parts": _parts(t, c, fill),
             "tasks": [_task(t, task, company, fill, _Checks(c.systems, kept.dropped)) for task in kept.tasks],
             "links": [fill(_without_needs(x), _at(t.data["links"], x, "links")) for x in kept.links],
             "gates": [_without_needs(g) for g in kept.gates],
             "instructions": fill(t.data.get("instructions", ""), "instructions"),
             "teams": fill(t.data.get("teams", []), "teams"),
             "events": [_event(t, e, c, ctx) for e in kept.events]}
    world.update(_passed_through(t))
    apply_systems(t, c, ctx, world, files)
    apply_events(t, kept.events, c, ctx, world, files)
    return world, files


def _at(items: list, item: dict, name: str) -> str:
    return f"{name}[{items.index(item)}]"


def _without_needs(item: dict) -> dict:
    return {k: copy.deepcopy(v) for k, v in item.items() if k != "needs"}


def _parts(t, c: Choices, fill) -> list:
    return [{"part": p["part"], "integration": p["integration"],
             "config": fill(p.get("config", {}), f"systems[{i}].parts[{j}].config")}
            for i, s in enumerate(t.systems) if s["id"] in c.systems for j, p in enumerate(s["parts"])]


@dataclass
class _Checks:
    """Which of a task's checks stay: those whose `needs` were all chosen; the rest go to `dropped`."""
    systems: list
    dropped: list

    def keep(self, task_id: str, checks: list) -> list:
        kept = []
        for item in checks:
            missing = [s for s in item.get("needs", []) if s not in self.systems]
            if missing:
                self.dropped.append({"what": f"check {item.get('name')} of task {task_id}",
                                     "why": f"needs {select.systems_phrase(missing)}, not chosen"})
            else:
                kept.append({k: v for k, v in item.items() if k != "needs"})
        return kept


def _task(t, task: dict, company: dict, fill, checks: _Checks) -> dict:
    path = _at(t.tasks, task, "tasks")
    out = {"id": task["id"], "reason": fill(task["reason"], f"{path}.reason"),
           "ticket": fill(task["ticket"], f"{path}.ticket"), "setup": fill(task.get("setup", []), f"{path}.setup"),
           "checks": fill(task["checks"], f"{path}.checks")}
    if not isinstance(out["checks"], list):
        raise TemplateError(f"{path}.checks", "checks must be a list")
    out["checks"] = checks.keep(task["id"], out["checks"])
    clock = task.get("clock_start") or company.get("clock_start")
    return {**out, "clock_start": clock} if clock else out


def _event(t, e: dict, c: Choices, ctx: Context) -> dict:
    chosen = c.events[e["id"]]
    label = expand(e["label"], replace(ctx, params=chosen["params"]), t.dir, [],
                   f"{_at(t.events, e, 'events')}.label")
    return {"id": e["id"], "label": label, "kind": e["kind"], "on": chosen["on"], "params": chosen["params"]}


def _passed_through(t) -> dict:
    out = {}
    if "instructions_title" in t.data:
        out["instructions_title"] = t.data["instructions_title"]
    if t.data.get("scenario_options") is not None:
        out["scenario_options"] = copy.deepcopy(t.data["scenario_options"])
    return out
