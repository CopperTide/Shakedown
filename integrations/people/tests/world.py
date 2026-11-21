"""A stand-in for shaked's part of a run: it advances each part on the clock, turns events into the link calls the
world declares (a string starting with `/` is a pointer into the event's data) and records every call as a step,
and every input each part got, in order. `Board` is a neutral twin on the kit that people ask and answer on."""
from dataclasses import dataclass, field

from shakedown_part.serve import dispatch
from shakedown_part.twin import Done, HttpTwin, Router
from people_part.part import People

from support import EPOCH, MIN

ACT_LINK = {"from": "people", "event": "board_act", "to": "board", "tool": "board_act",
            "args": {"person": "/person", "action": "/action", "args": "/args"}}
NOTICE_LINK = {"from": "board", "event": "notice", "to": "people", "tool": "people_notice",
               "args": {"system": "/system", "kind": "/kind", "data": "/data"}}
LINKS = [ACT_LINK, NOTICE_LINK]
ORDER = ("people", "board")


class Board(HttpTwin):
    NAME, SYSTEM = "board-twin", "board"
    SCHEMA_SQL = "CREATE TABLE IF NOT EXISTS posts(id INTEGER PRIMARY KEY, by TEXT, kind TEXT, topic TEXT, text TEXT);"

    def router(self):
        return Router()

    def docs(self, ctx):
        rows = ctx.store.conn.execute("SELECT * FROM posts ORDER BY id").fetchall()
        return {f"post/{r['id']}": {"by": r["by"], "kind": r["kind"], "topic": r["topic"], "text": r["text"]}
                for r in rows}

    def act(self, ctx, person, action, args):
        if action not in ("ask", "answer"):
            return Done(error=f"unknown action {action}")
        db = ctx.store.conn
        n = db.execute("INSERT INTO posts(by, kind, topic, text) VALUES (?, ?, ?, ?)",
                       (person, action, args.get("topic"), args.get("text"))).lastrowid
        if action == "ask":
            ctx.notify("asked", f"{person} asked {args['to']} about {args['topic']}",
                       {"user": args["to"], "topic": args["topic"], "by": person})
        return Done({"id": n}, writes=[f"post/{n}"])


def pointer(data, ptr: str):
    for token in ptr.lstrip("/").split("/") if ptr != "/" else []:
        data = data[int(token)] if isinstance(data, list) else data[token]
    return data


@dataclass
class World:
    parts: dict  # name -> (part, instance)
    clock: int = 0
    steps: list = field(default_factory=list)
    inputs: dict = field(default_factory=lambda: {name: [] for name in ORDER})

    @classmethod
    def start(cls, people_config, saves=None, clock_ms=0, suffix=""):
        parts = {"people": (People(), f"p{suffix}"), "board": (Board(), f"b{suffix}")}
        configs = {"people": people_config, "board": {"hosts": [{"host": "board.acme.test"}], "default_user": "x"}}
        for name, (part, instance) in parts.items():
            params = {"instance": instance, "config": configs[name], "seed": 1, "clock_ms": clock_ms,
                      "world_time": {"epoch_ms": EPOCH, "speed": 1.0}}
            if saves:
                params["from"] = {"save": saves[name], "mode": "exact"}
            dispatch(part, "start", params)
        return cls(parts, clock_ms)

    def run(self, until_ms: int, step_ms: int = MIN) -> None:
        while self.clock < until_ms:
            self.clock = min(self.clock + step_ms, until_ms)
            for name in ORDER:
                part, instance = self.parts[name]
                out = dispatch(part, "advance", {"instance": instance, "clock_ms": self.clock})
                self.inputs[name].append(("advance", self.clock, out))
                self._links(name, out["events"])

    def _links(self, source: str, events: list) -> None:
        for e in events:
            for link in (x for x in LINKS if x["from"] == source and x["event"] == e["kind"]):
                args = {k: pointer(e["data"], v) if isinstance(v, str) and v.startswith("/") else v
                        for k, v in link["args"].items()}
                part, instance = self.parts[link["to"]]
                out = dispatch(part, "call_tool", {"instance": instance, "name": link["tool"], "args": args,
                                                         "clock_ms": self.clock})
                self.inputs[link["to"]].append(("call", self.clock, link["tool"], args, out))
                self.steps.append({"actor": "link", "clock_ms": self.clock, "part": link["to"],
                                   "tool": link["tool"], "args": args, "result": out.get("result")})
                self._links(link["to"], out.get("events", []))

    def save(self, save: str, saves_root) -> dict:
        names = {}
        for name, (part, instance) in self.parts.items():
            folder = f"{save}_{name}"
            part.save(instance, folder, str(saves_root / part.NAME / folder))
            names[name] = folder
        return names

    def stop(self) -> None:
        for part, instance in self.parts.values():
            part.stop(instance)

