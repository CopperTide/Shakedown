"""The world builder program: `templates`, `template`, `preview` and `generate` over the line protocol
(docs/protocol.md, "Builder calls"). Templates come from the package's own `templates/`, the roots its command
names (`--templates DIR`, repeatable; the manifest names `library`, the templates shipped with it) and the
`:`-separated `SHAKEDOWN_TEMPLATES` roots."""
import argparse
import os
from pathlib import Path

from shakedown_part import errors, serve
from shakedown_part.errors import PartError
from shakedown_part.serve import _need

from . import install
from .preview import assess, preview
from .template import Template, form, list_templates
from shakedown_part.settings import setting

PACKAGE_TEMPLATES = Path(__file__).resolve().parents[1] / "templates"


class Builder:
    """Answers the builder calls over the templates under `roots`, read again on every call."""

    def __init__(self, roots: list[Path]):
        self.roots = list(roots)

    @classmethod
    def from_env(cls, named: tuple = ()) -> "Builder":
        extra = [Path(p) for p in setting("SHAKEDOWN_TEMPLATES", "").split(":") if p]
        return cls([PACKAGE_TEMPLATES, *map(Path, named), *extra])

    def find(self, id: str) -> Template:
        found = next((t for t in list_templates(self.roots) if t.id == id), None)
        if found is None:
            raise errors.invalid(f"no template {id}")
        return found

    def templates(self) -> list:
        return [{"id": t.id, "name": t.name, "summary": t.summary, "version": t.version,
                 "systems": [s["label"] for s in t.systems]} for t in list_templates(self.roots)]

    def template(self, id: str) -> dict:
        return form(self.find(id))

    def preview(self, id: str, choices: dict) -> dict:
        return preview(self.find(id), choices)

    def generate(self, id: str, choices: dict, dir: str) -> dict:
        target = Path(dir)
        if target.exists():
            raise errors.invalid(f"{target} exists already")
        t = self.find(id)
        a = assess(t, choices)
        if a.problems:
            first = a.problems[0]
            where = f"{first['path']}: " if first["path"] else ""
            raise PartError(-32602, f"the world can't be built: {where}{first['message']}",
                            data={"problems": a.problems})
        return install.write_package(a.result, t, target)


_METHODS = {
    "templates": lambda b, p: b.templates(),
    "template": lambda b, p: b.template(*_need(p, "id")),
    "preview": lambda b, p: b.preview(*_need(p, "id", "choices")),
    "generate": lambda b, p: b.generate(*_need(p, "id", "choices", "dir")),
}


def dispatch(builder: Builder, method: str, params):
    handler = _METHODS.get(method)
    if handler is None:
        raise errors.method_not_found(method)
    return handler(builder, params)


def main() -> None:
    args = argparse.ArgumentParser(prog="worldbench.builder")
    args.add_argument("--templates", action="append", default=[], help="a folder of templates (repeatable)")
    serve(Builder.from_env(tuple(args.parse_args().templates)), dispatch_fn=dispatch)


if __name__ == "__main__":
    main()
