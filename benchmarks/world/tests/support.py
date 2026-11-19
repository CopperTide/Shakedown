"""Helpers the tests share: edited copies of fixture templates and expansion with any context."""
import shutil
from pathlib import Path

import yaml

from worldbench.expand import Context, expand


def _set(data, dotted: str, value):
    *head, last = dotted.split(".")
    for k in head:
        data = data[int(k)] if isinstance(data, list) else data[k]
    if isinstance(data, list):
        data[int(last)] = value
    else:
        data[last] = value


def copy_with(src: Path, tmp: Path, changes: dict, drop: tuple = ()) -> Path:
    """A copy of the template folder `src` under `tmp` with dotted paths of its template.yaml set to values
    (and the dotted paths in `drop` removed)."""
    dst = tmp / src.name
    shutil.copytree(src, dst)
    data = yaml.safe_load((dst / "template.yaml").read_text())
    for k, v in changes.items():
        _set(data, k, v)
    for k in drop:
        *head, last = k.split(".")
        parent = data
        for h in head:
            parent = parent[int(h)] if isinstance(parent, list) else parent[h]
        del parent[int(last) if isinstance(parent, list) else last]
    (dst / "template.yaml").write_text(yaml.safe_dump(data, sort_keys=False))
    return dst


def ctx_any() -> Context:
    company = {"name": "Acme", "slug": "acme", "key": "AC", "domain": "acme.test"}
    return Context(company=company, size=3, options={}, people=[], params={})


def expand_all(t):
    """Expands the whole template with any context (raises its first TemplateError)."""
    return expand(t.data, ctx_any(), t.dir, [])
