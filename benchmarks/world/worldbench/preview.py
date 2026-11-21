"""What the builder's Review step shows of a world before it is built, and what blocks building it."""
from dataclasses import dataclass

from . import needs
from .choices import Choices, problem
from .errors import TemplateError
from .expand import Context, expand
from .generate import Result, company_of, generate


@dataclass
class Assessed:
    """A world generated from choices, with what it needs from the machine and every problem (the generation's,
    the hosts', and each missing image)."""
    result: Result
    choices: Choices
    company: dict
    needs: dict
    hosts: list
    problems: list


def assess(t, raw_choices: dict) -> Assessed:
    result = generate(t, raw_choices)
    c, _ = Choices.parse(t, raw_choices)
    company = company_of(t, c.company)
    nd = needs.of(c.systems, t)
    hosts, host_problems = _hosts(t, c, company)
    problems = result.problems + host_problems + needs.missing(nd)
    return Assessed(result, c, company, nd, hosts, problems)


def _hosts(t, c: Choices, company: dict) -> tuple[list, list]:
    ctx = Context(company=company, size=c.size, options=c.options, people=[], params={}, seed=c.seed)
    hosts = []
    for i, s in enumerate(t.systems):
        if s["id"] not in c.systems:
            continue
        try:
            hosts += expand(s.get("hosts", []), ctx, t.dir, [], f"systems[{i}].hosts")
        except TemplateError as e:
            return [], [problem(e.path, e.message)]
    return hosts, []


def preview(t, raw_choices: dict) -> dict:
    """The preview answer (docs/protocol.md, "Builder calls")."""
    a = assess(t, raw_choices)
    w = a.result.world or {}
    return {"name": a.company["slug"], "title": a.choices.company, "domain": a.company["domain"],
            "parts": [{"part": p["part"], "integration": p["integration"], "system": s["id"]}
                      for s in t.systems if s["id"] in a.choices.systems for p in s["parts"]],
            "hosts": a.hosts, "people": a.result.people,
            "events": [{k: e[k] for k in ("id", "label", "kind", "on")} for e in w.get("events", [])],
            "tasks": [{"id": x["id"], "reason": x["reason"], "checks": len(x["checks"])} for x in w.get("tasks", [])],
            "dropped": a.result.dropped, "needs": a.needs, "problems": a.problems}
