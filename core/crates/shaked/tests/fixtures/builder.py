"""A toy world builder for shaked tests (builder calls, docs/protocol.md "Builder calls").

  builder.py --toy TOY_PY --integration NAME [--bench-arg ARG ...]
      One template `toy` (systems: `box`). `preview` answers the slug of the company's name and a problem
      `Give the company a name` when it is blank; `generate` refuses the same problem with `data: {problems}`
      and otherwise writes a benchmark package into `dir`: `opt_in`, a `[world]` table, and a command running
      `python3 TOY_PY bench --name <slug> --integration NAME <--bench-arg values>`.
"""
import json
import os
import re
import sys

ARGS = sys.argv[1:]


class RpcError(Exception):
    def __init__(self, code, message, data=None):
        super().__init__(message)
        self.code, self.message, self.data = code, message, data


def opt(name):
    return ARGS[ARGS.index(name) + 1]


def opts(name):
    return [ARGS[i + 1] for i, a in enumerate(ARGS) if a == name]


FORM = {"id": "toy", "name": "Toy", "summary": "One box", "version": "1.0.0", "company": {"name": "Toy Co"},
        "size": {"label": "Boxes", "default": 1, "min": 1, "max": 3}, "systems": [], "roles": [], "events": []}


def company(choices):
    return ((choices.get("company") or {}).get("name", FORM["company"]["name"])).strip()


def slug(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def problems(choices):
    return [] if company(choices) else [{"path": "company.name", "message": "Give the company a name"}]


def template_of(p):
    if p.get("id") != "toy":
        raise RpcError(-32602, "no template %s" % p.get("id"))
    return FORM


def preview(p):
    template_of(p)
    c = p.get("choices") or {}
    name = company(c)
    return {"name": slug(name), "title": name, "domain": slug(name) + ".test", "parts": [], "hosts": [],
            "people": [], "events": [], "tasks": [{"id": "t1", "reason": "poke it", "checks": 1}], "dropped": [],
            "needs": {"images": [], "memory_mb": 1}, "problems": problems(c)}


def manifest(name, title):
    command = ["python3", opt("--toy"), "bench", "--name", name, "--integration", opt("--integration")]
    command += opts("--bench-arg")
    return "\n".join([
        'kind = "benchmark"', "name = %s" % json.dumps(name), 'version = "1.0.0+0"', "protocol = 1",
        "opt_in = true", "run = { command = %s }" % json.dumps(command), "", "[world]",
        "title = %s" % json.dumps(title), 'template = "toy"', 'template_name = "Toy"', ""])


def generate(p):
    found = problems(p.get("choices") or {})
    if found:
        raise RpcError(-32602, "the world can't be built: company.name: Give the company a name", {"problems": found})
    if os.path.exists(p["dir"]):
        raise RpcError(-32602, "%s exists already" % p["dir"])
    shown = preview(p)
    os.makedirs(p["dir"])
    with open(os.path.join(p["dir"], "arena-package.toml"), "w") as f:
        f.write(manifest(shown["name"], shown["title"]))
    return {"name": shown["name"], "title": shown["title"], "version": "1.0.0+0"}


METHODS = {
    "describe": lambda p: {"name": "toy-builder", "version": "0.1.0", "protocol": 1},
    "templates": lambda p: [{k: FORM[k] for k in ("id", "name", "summary", "version")} | {"systems": []}],
    "template": template_of,
    "preview": preview,
    "generate": generate,
}


def answer(request):
    out = {"jsonrpc": "2.0", "id": request.get("id")}
    try:
        method = METHODS.get(request.get("method"))
        if method is None:
            raise RpcError(-32601, "method not found: %s" % request.get("method"))
        out["result"] = method(request.get("params") or {})
    except RpcError as e:
        out["error"] = {"code": e.code, "message": e.message}
        if e.data is not None:
            out["error"]["data"] = e.data
    return out


for line in sys.stdin:
    if line.strip():
        print(json.dumps(answer(json.loads(line))), flush=True)
