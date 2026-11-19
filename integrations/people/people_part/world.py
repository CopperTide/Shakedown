"""The people part's config checked and compiled: the people, each role's rules and the systems the world has."""
import json
from dataclasses import dataclass
from functools import lru_cache

from . import rules

PERSON_FIELDS = ("id", "name", "login", "role")


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class World:
    people: tuple  # of person dicts, in config order
    rules: dict  # role -> RoleRules
    systems: frozenset | None  # None: every system

    def by_id(self, person_id: str) -> dict | None:
        return next((p for p in self.people if p["id"] == person_id), None)

    def of_role(self, role: str) -> list:
        return [p for p in self.people if p["role"] == role]

    def rule(self, role: str, rule_id: str):
        found = self.rules.get(role)
        return next((r for r in found.rules if r.id == rule_id), None) if found else None

    def roles(self) -> list:
        """The roles with rules, sorted, so nothing depends on how the config's mapping was ordered."""
        return sorted(self.rules)


def compile_config(config: dict) -> World:
    """The world of `config` (`{seed?, people, rules: {role: doc}, systems?}`); ConfigError when it doesn't fit."""
    return _compiled(json.dumps(config, sort_keys=True))


@lru_cache(maxsize=32)
def _compiled(text: str) -> World:
    config = json.loads(text)
    if not isinstance(config, dict):
        raise ConfigError("the config must be an object")
    systems = config.get("systems")
    if systems is not None and not (isinstance(systems, list) and all(isinstance(s, str) for s in systems)):
        raise ConfigError("systems must be a list of names")
    return World(_people(config.get("people")), _rules(config.get("rules")),
                 frozenset(systems) if systems is not None else None)


def _people(people) -> tuple:
    if not isinstance(people, list):
        raise ConfigError("people must be a list")
    for i, p in enumerate(people):
        if not isinstance(p, dict) or not all(isinstance(p.get(f), str) and p[f] for f in PERSON_FIELDS):
            raise ConfigError(f"people[{i}] needs {', '.join(PERSON_FIELDS)}")
    for field in ("id", "login"):
        seen = [p[field] for p in people]
        twice = sorted({v for v in seen if seen.count(v) > 1})
        if twice:
            raise ConfigError(f"people: the {field} {twice[0]} is used twice")
    return tuple(people)


def _rules(given) -> dict:
    if not isinstance(given, dict):
        raise ConfigError("rules must map each role to its rules")
    out = {}
    for role, doc in given.items():
        loaded = rules.load(doc)
        if isinstance(loaded, rules.RulesError):
            raise ConfigError(f"rules.{role}: {loaded}")
        if loaded.role != role:
            raise ConfigError(f"rules.{role}: the doc is for the role {loaded.role}")
        out[role] = loaded
    return out
