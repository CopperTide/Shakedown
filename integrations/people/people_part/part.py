"""The shakedown-part/1 integration: simulated people on the world clock. Its config names the people, each role's
rules (`shakedown-people/1`) and the systems the world has. People never call a system: the world's links hand
every notice to `people_notice` and every act event (`<system>_act {person, action, args}`) to that system."""
from shakedown_part import errors
from shakedown_part.jsonstate import ClockedJsonStatePart, Done, Tool
from shakedown_part.params import Param

from . import docs, engine
from .rules import FORMAT
from .world import ConfigError, compile_config

VERSION = "0.1.0"


def _notice(config, state, args) -> Done:
    world = compile_config(config)
    touched, queued = engine.notice(world, state, args["system"], args["kind"], args["data"] or {})
    return Done({"queued": queued}, writes=[f"person/{pid}" for pid in touched])


TOOLS = {"people_notice": Tool(
    "people_notice", "A system's notice to the world's people (called by links): the rules it sets off are queued.",
    "write", (Param("system", "string"), Param("kind", "string"), Param("data", "object", None)), _notice)}


class People(ClockedJsonStatePart):
    NAME = "people"
    TOOLS = TOOLS
    WORLD_TOOLS = frozenset(TOOLS)

    def describe_fields(self):
        return {"version": VERSION, "fingerprint": {"adapter": VERSION, "format": FORMAT},
                "system": {"kind": "people", "product": "Simulated people acting by rules written as data",
                           "stands_in_for": "the people of the world", "simulated": True}}

    def new_state(self, config):
        try:
            compile_config(config)
        except ConfigError as e:
            raise errors.invalid(f"invalid config: {e}") from None
        return {"clock_ms": 0, "queued": []}

    def start(self, instance, config, seed, clock_ms, from_=None):
        super().start(instance, config, seed, clock_ms, from_)
        if from_ is None:
            config = config or {}
            w = self.world_time(instance)
            engine.begin(compile_config(config), self._instance(instance).state, config.get("seed", seed),
                         w.epoch_ms if w is not None else 0, clock_ms)

    def world_of(self, instance):
        """The compiled world (people, rules, systems) of a live instance."""
        return compile_config(self._instance(instance).config)

    def step(self, config, state, clock_ms) -> list:
        return engine.run_to(compile_config(config), state, clock_ms)

    def docs(self, config, state):
        return docs.docs(compile_config(config), state)
