"""Templates: loading one from its folder (schema-checked), listing those under some roots, and the form the UI
renders from one."""
import sys
from dataclasses import dataclass
from pathlib import Path

import yaml

from . import validate
from .errors import TemplateError

__all__ = ["Template", "TemplateError", "form", "list_templates", "load_template"]


@dataclass(frozen=True)
class Template:
    """A loaded template: its identity, its folder and its parsed (not expanded) template.yaml."""
    id: str
    version: str
    name: str
    summary: str
    dir: Path
    data: dict

    @property
    def systems(self) -> list:
        return self.data["systems"]

    @property
    def roles(self) -> list:
        return self.data.get("roles", [])

    @property
    def events(self) -> list:
        return self.data.get("events", [])

    @property
    def tasks(self) -> list:
        return self.data["tasks"]


def load_template(folder: Path) -> Template:
    """The template in `folder`; TemplateError when it is missing, unparsable or fails a check."""
    file = folder / "template.yaml"
    if not file.is_file():
        raise TemplateError("", f"no template.yaml in {folder}")
    try:
        data = yaml.safe_load(file.read_text())
    except yaml.YAMLError as e:
        raise TemplateError("", f"template.yaml can't be parsed: {e}") from None
    validate.check(data, folder)
    return Template(id=data["id"], version=data["version"], name=data["name"], summary=data["summary"],
                    dir=folder, data=data)


def list_templates(roots: list[Path]) -> list[Template]:
    """Every loadable template under `roots`, sorted by id; a bad one (or a second of one id) is logged and
    skipped."""
    found: dict[str, Template] = {}
    for folder in _folders(roots):
        try:
            t = load_template(folder)
        except TemplateError as e:
            print(f"worldbench: skipping the template in {folder}: {e}", file=sys.stderr)
            continue
        if t.id in found:
            print(f"worldbench: skipping {folder}: template {t.id} is in {found[t.id].dir}", file=sys.stderr)
            continue
        found[t.id] = t
    return [found[k] for k in sorted(found)]


def _folders(roots: list[Path]):
    for root in roots:
        if root.is_dir():
            yield from (d for d in sorted(root.iterdir()) if (d / "template.yaml").is_file())


def form(t: Template) -> dict:
    """What the builder's UI renders: the template's choices with their defaults (no parts, no data)."""
    return {"id": t.id, "name": t.name, "summary": t.summary, "version": t.version,
            "company": {"name": t.data["company"]["name"]}, "size": dict(t.data["size"]),
            "systems": [_system_form(s) for s in t.systems], "roles": [_role_form(r) for r in t.roles],
            "events": [_event_form(e) for e in t.events]}


def _with_summary(out: dict, item: dict) -> dict:
    return {**out, "summary": item["summary"]} if "summary" in item else out


def _system_form(s: dict) -> dict:
    return _with_summary({"id": s["id"], "label": s["label"], "product": s["product"],
                          "required": s.get("required", False), "default": s.get("default", False),
                          "requires": s.get("requires", []), "memory_mb": s.get("memory_mb", 0),
                          "hosts": s.get("hosts", []), "options": s.get("options", [])}, s)


def _role_form(r: dict) -> dict:
    return _with_summary({"id": r["id"], "label": r["label"], "count": r["count"], "min": r.get("min", 0),
                          "systems": r["systems"]}, r)


def _event_form(e: dict) -> dict:
    return {"id": e["id"], "label": e["label"], "description": e["description"], "kind": e["kind"],
            "default": e.get("default", False), "needs": e.get("needs", []), "params": e.get("params", [])}
