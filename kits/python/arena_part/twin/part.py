"""`HttpTwin`: a part whose instances are web services (a SaaS twin). Each instance has its own
HTTP server, an SQLite store and a file tree; it is saved at save points and restored exactly."""
import json
import shutil
import sys
import threading
import time
import traceback
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from .. import errors, saves
from ..docspart import DocsPart
from .router import Router
from .server import TwinServer
from .store import Store
from .trees import link_copy

STORE_FILE = "store.sqlite"
FILES_DIR = "files"
CONFIG_FILE = "config.json"


@dataclass
class Done:
    """An act's answer: its result, or the refusal the caller sees, and the keys it wrote."""
    result: object = None
    error: str | None = None
    writes: list = field(default_factory=list)

    def answer(self) -> dict:
        if self.error is not None:
            return {"error": self.error, "reads": [], "writes": []}
        return {"result": self.result, "reads": [], "writes": list(self.writes)}


@dataclass
class TwinCtx:
    """What handlers, seeds, docs and acts of one instance share. Hold `lock` to touch it."""
    store: Store
    files: Path
    config: dict
    system: str
    now_ms: Callable[[], int]
    lock: threading.RLock = field(default_factory=threading.RLock)
    notices: list = field(default_factory=list)
    undo: list = field(default_factory=list)
    changed: bool = False

    def actor(self, req) -> str:
        """Who a request acts as: its agent, else the config's default user."""
        return req.agent or self.config.get("default_user", "")

    def notify(self, kind: str, text: str, data: dict) -> None:
        """Queue a `notice` event, reported at the next advance."""
        self.event("notice", text, {"system": self.system, "kind": kind, "data": data})

    def event(self, kind: str, text: str, data: dict) -> None:
        """Queue a world event of `kind` (links carry it), reported at the next advance; dropped with
        the request or act it is queued in when that fails."""
        self.notices.append({"kind": kind, "text": text, "data": data})

    def on_rollback(self, undo: Callable[[], None]) -> None:
        """Have `undo` run if the request, act or savepoint this runs in is rolled back: for work
        outside the store (files, git refs)."""
        self.undo.append(undo)

    def _undo_to(self, mark: int) -> None:
        while len(self.undo) > mark:
            try:
                self.undo.pop()()
            except Exception:
                traceback.print_exc(file=sys.stderr)

    @contextmanager
    def atomic(self):
        """One request or act: its store transaction; on a raise its notices are dropped and its
        outside work undone, newest first."""
        with self._undoing():
            with self.store.tx():
                yield self.store.conn

    @contextmanager
    def savepoint(self, name: str = "part"):
        """A nested part of a request that may fail on its own: on a raise, what it did is undone as
        `atomic` does, and the raise goes on."""
        db = self.store.conn
        db.execute(f"SAVEPOINT {name}")
        try:
            with self._undoing():
                yield db
        except BaseException:
            db.execute(f"ROLLBACK TO {name}")
            raise
        finally:
            db.execute(f"RELEASE {name}")

    @contextmanager
    def _undoing(self):
        queued, mark = len(self.notices), len(self.undo)
        try:
            yield
        except BaseException:
            del self.notices[queued:]
            self._undo_to(mark)
            raise
        del self.undo[mark:]


def _wall_ms() -> int:
    return time.time_ns() // 1_000_000


class HttpTwin(DocsPart):
    """Subclass and set `NAME` (the package name), `SYSTEM` (names the `<SYSTEM>_act` tool and
    notices), `VERSION`, `SCHEMA_SQL`; give `router()`, `seed`, `docs`, `act`; override
    `is_immutable(path)` for files that never change in place, `route_env`, `READS`, `world_tools`
    for world tools beside `<SYSTEM>_act`; set `HONOURS_READONLY` when the twin refuses the writes of a
    request marked `req.readonly`."""
    NAME = ""
    SYSTEM = ""
    VERSION = "0.1.0"
    SCHEMA_SQL = ""
    READS: list = []
    HONOURS_READONLY = False

    def __init__(self, clock: Callable[[], int] | None = None):
        super().__init__()
        self._wall = clock or _wall_ms
        self._servers: dict[str, TwinServer] = {}
        self._router = None

    # --- what a twin gives --------------------------------------------------------------
    def router(self) -> Router:
        raise NotImplementedError("router")

    def seed(self, ctx: TwinCtx, config: dict) -> None:
        pass

    def docs(self, ctx: TwinCtx) -> dict:
        return {}

    def act(self, ctx: TwinCtx, person: str, action: str, args: dict) -> Done:
        return Done(error=f"unknown action {action}")

    def is_immutable(self, path: Path) -> bool:
        return False

    def route_env(self, config: dict, host: dict) -> dict:
        return dict(host.get("env") or {})

    def world_tools(self) -> dict:
        """More world tools (called by links): `{name: (tool, run(ctx, args) -> Done)}`."""
        return {}

    def act_tool(self) -> dict:
        return {"name": f"{self.SYSTEM}_act", "effect": "write",
                "description": f"Act in {self.SYSTEM} as one of the world's people (called by links).",
                "inputSchema": {"type": "object", "required": ["person", "action"], "properties": {
                    "person": {"type": "string"}, "action": {"type": "string"}, "args": {"type": "object"}}}}

    # --- the protocol -------------------------------------------------------------------
    def describe(self):
        return {"name": self.NAME, "version": self.VERSION, "protocol": 1,
                "restore": {"exact": "save", "approximate": False}, "uses_clock": True,
                "fingerprint": {"schema": self.SCHEMA_SQL.strip()[:64]},
                "tools": [self.act_tool(), *(tool for tool, _ in self.world_tools().values())],
                "save_at": "points"}

    def ctx(self, instance) -> TwinCtx:
        return self._instance(instance)

    def now_ms(self, instance) -> int:
        """World ms (unix) when arenad gave the world's time at `start`, else the wall clock."""
        w = self.world_time(instance)
        return w.now_ms() if w is not None else self._wall()

    def _live_dir(self, instance) -> Path:
        return saves.root(self.NAME) / ".live" / instance

    def start(self, instance, config, seed, clock_ms, from_=None):
        if instance in self.instances:
            self.stop(instance)
        live = self._live_dir(instance)
        shutil.rmtree(live, ignore_errors=True)
        live.mkdir(parents=True)
        ctx = TwinCtx(Store(live / STORE_FILE, self.SCHEMA_SQL), live / FILES_DIR, dict(config),
                      self.SYSTEM, lambda: self.now_ms(instance))
        self.instances[instance] = ctx
        if from_ is not None:
            self._restore(ctx, from_["save"])
        else:
            ctx.files.mkdir()
            with ctx.lock, ctx.store.tx():
                self.seed(ctx, ctx.config)
        self._servers[instance] = TwinServer(self._routes(), ctx).start()

    def _routes(self) -> Router:
        if self._router is None:
            self._router = self.router()
        return self._router

    def _restore(self, ctx: TwinCtx, save: str) -> None:
        src = saves.root(self.NAME) / save
        if not (src / STORE_FILE).is_file():
            raise errors.unknown(f"unknown save {save}")
        ctx.store.restore_from(src / STORE_FILE)
        link_copy(src / FILES_DIR, ctx.files, self.is_immutable)

    def http_routes(self, instance):
        ctx, server = self._instance(instance), self._servers[instance]
        out = []
        for host in ctx.config.get("hosts", []):
            route = {"host": host["host"], "upstream": server.upstream}
            env = self.route_env(ctx.config, host)
            if env:
                route["env"] = env
            if self.READS:
                route["reads"] = list(self.READS)
            if self.HONOURS_READONLY:
                route["honours_readonly"] = True
            out.append(route)
        return {"routes": out}

    def call_tool(self, instance, name, args, clock_ms):
        ctx = self._instance(instance)
        run = self._tool(name)
        if run is None:
            return {"error": f"unknown tool {name}", "reads": [], "writes": []}
        with ctx.lock:
            try:
                with ctx.atomic():
                    done = run(ctx, args)
                    if done.error is not None:
                        raise _Refused(done)
            except _Refused as r:
                return r.done.answer()
            ctx.changed = True
            return done.answer()

    def _tool(self, name: str):
        if name == f"{self.SYSTEM}_act":
            return lambda ctx, args: self.act(ctx, args.get("person", ""), args.get("action", ""), args.get("args") or {})
        found = self.world_tools().get(name)
        return found[1] if found else None

    def advance(self, instance, clock_ms):
        ctx = self._instance(instance)
        with ctx.lock:
            events, ctx.notices = ctx.notices, []
            changed, ctx.changed = ctx.changed, False
        return {"changed": changed, "events": events}

    def save(self, instance, save, dir):
        ctx, dst = self._instance(instance), Path(dir)
        dst.mkdir(parents=True, exist_ok=True)
        with ctx.lock:
            n = ctx.store.backup_to(dst / STORE_FILE)
            c = link_copy(ctx.files, dst / FILES_DIR, self.is_immutable)
            (dst / CONFIG_FILE).write_text(json.dumps(ctx.config))
        return {"bytes": n + c.bytes, "exact": True}

    def live_docs(self, instance) -> dict:
        ctx = self._instance(instance)
        with ctx.lock:
            return self.docs(ctx)

    def saved_docs(self, save_dir: Path) -> dict:
        if not (save_dir / STORE_FILE).is_file():
            raise errors.unknown(f"unknown save {save_dir.name}")
        store = Store(save_dir / STORE_FILE, readonly=True)
        try:
            config = json.loads((save_dir / CONFIG_FILE).read_text())
            return self.docs(TwinCtx(store, save_dir / FILES_DIR, config, self.SYSTEM, self._wall))
        finally:
            store.close()

    def stop(self, instance):
        server, ctx = self._servers.pop(instance, None), self.instances.pop(instance, None)
        if server is not None:
            server.close()
        if ctx is not None:
            ctx.store.close()
        shutil.rmtree(self._live_dir(instance), ignore_errors=True)


class _Refused(Exception):
    def __init__(self, done: Done):
        self.done = done
