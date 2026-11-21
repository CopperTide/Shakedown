"""The shakedown-part/1 integration: one World per instance, saved as one JSON file."""
from shakedown_part import DocsPart, errors, saves, sources

from . import state
from .config import parse_config, source_files, source_hashes
from .tools import run_tool, schemas
from .world import TICK_MS, World

NAME = "ridgeline-hub"
SEED_NOTE = ("accepted and unused: the market has no randomness, so the replay contract holds for any seed")


def check_fingerprint(cfg):
    """The configured source files must be there and be the ones `describe` declared, or replays would
    differ. Checked before importing them, so a refused start leaves nothing loaded."""
    sources.check_unchanged(source_hashes(parse_config({})), source_files(cfg), "check hub_path and data_path")


class RidgelineHub(DocsPart):
    NAME = NAME  # self.instances: {id: World}

    def live_docs(self, instance) -> dict:
        return state.docs_of(state.snapshot(self._instance(instance)))

    def saved_docs(self, save_dir) -> dict:
        return state.docs_of(state.read(save_dir))

    def describe(self):
        return {
            "name": NAME, "version": "0.1.0", "protocol": 1,
            "restore": {"exact": "save", "approximate": False},
            "system": {"kind": "logistics", "product": "Ridgeline hub (simulated suppliers, carriers, customers)", "simulated": True},
            "uses_clock": True,
            "fingerprint": {"adapter": 1, "tick_ms": TICK_MS, "seed": SEED_NOTE,
                            "sha256": source_hashes(parse_config({}))},
            "tools": schemas(),
        }

    def start(self, instance, config, seed, clock_ms, from_=None):
        cfg = parse_config(config)
        check_fingerprint(cfg)
        world = World(cfg, clock_ms)
        if from_ is not None:  # exact saves also serve approximate restores
            if not isinstance(from_, dict) or "save" not in from_:
                raise errors.invalid("invalid params: from needs a save")
            state.restore(world, state.read(saves.root(NAME) / from_["save"]))
        self.instances[instance] = world

    def call_tool(self, instance, name, args, clock_ms):
        world = self._instance(instance)
        world.step_to(clock_ms)
        return run_tool(world, name, args)

    def advance(self, instance, clock_ms):
        events = self._instance(instance).advance(clock_ms)
        return {"changed": bool(events), "events": events}

    def save(self, instance, save, dir):
        size = state.write(dir, state.snapshot(self._instance(instance)))
        return {"bytes": size, "exact": True}
