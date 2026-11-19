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


def _reply(builder, method: str, params: dict) -> dict:
    import io
    import json

    from arena_part import serve
    from worldbench.builder import dispatch
    out = io.StringIO()
    line = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    serve(builder, io.StringIO(line + "\n"), out, dispatch_fn=dispatch)
    return json.loads(out.getvalue())


def call(builder, method: str, params: dict):
    """The builder's result for one request over the line protocol."""
    reply = _reply(builder, method, params)
    assert "error" not in reply, reply
    return reply["result"]


def call_error(builder, method: str, params: dict) -> dict:
    """The builder's error for one request over the line protocol."""
    reply = _reply(builder, method, params)
    assert "error" in reply, reply
    return reply["error"]
