"""The traffic's settings: the data file (`journeys.json`: rates, the day's shape, journeys, events, services) and
what the environment adds: `TRAFFIC_SEED`, `TRAFFIC_EVENTS` (the builder's toggles: `name` or `name@<minute>`,
comma-separated), `TRAFFIC_SERVICES` (`name=url,...` overrides), `OTEL_EXPORTER_OTLP_ENDPOINT`,
`SHAKEDOWN_CLOCK_FILE`, `TRAFFIC_CONTRACTS` and `TRAFFIC_PORT`."""
import json
from dataclasses import dataclass
from pathlib import Path

MINUTE_MS = 60_000


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class Config:
    data: dict
    seed: int | str
    profile: dict
    services: dict
    otlp_endpoint: str | None
    clock_file: Path
    contracts_dir: Path
    port: int

    def __getattr__(self, name):
        """The data file's plain settings (`tick_ms`, `timeout_s`, `workers`, ...) read as attributes."""
        try:
            return self.__dict__["data"][name]
        except KeyError:
            raise AttributeError(name) from None


def peaks(events: dict, toggles: str) -> list[dict]:
    out = []
    for toggle in filter(None, (t.strip() for t in toggles.split(","))):
        name, _, minute = toggle.partition("@")
        if name not in events:
            raise ConfigError(f"no traffic event {name} (the data has {', '.join(sorted(events)) or 'none'})")
        e = events[name]
        start = int(minute or e["at_minute"]) * MINUTE_MS
        out.append({"from_ms": start, "to_ms": start + e["for_minutes"] * MINUTE_MS, "factor": e["factor"]})
    return out


def check(data: dict) -> None:
    if len(data["day"]["hourly"]) != 24:
        raise ConfigError("the day's shape needs 24 hourly values")
    for name, journey in data["journeys"].items():
        for n, step in enumerate(journey["steps"], 1):
            if step["service"] not in data["services"]:
                raise ConfigError(f"journey {name} step {n} names the service {step['service']}, not one listed")


def seed_of(text: str, default: int) -> int | str:
    """The builder passes the world's seed as text (`world-7`); a number stays a number, so its schedule is the same
    as before."""
    text = text.strip()
    if not text:
        return default
    return int(text) if text.isdigit() else text


def overrides(text: str) -> dict:
    return dict(pair.strip().split("=", 1) for pair in text.split(",") if pair.strip())


def load(path: Path, env: dict) -> Config:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    check(data)
    profile = {"journeys": {n: {"rpm": j["rpm"]} for n, j in data["journeys"].items()}, "day": data["day"],
               "peaks": peaks(data.get("events", {}), env.get("TRAFFIC_EVENTS", ""))}
    return Config(data=data, seed=seed_of(env.get("TRAFFIC_SEED", ""), data["seed"]), profile=profile,
                  services={**data["services"], **overrides(env.get("TRAFFIC_SERVICES", ""))},
                  otlp_endpoint=env.get("OTEL_EXPORTER_OTLP_ENDPOINT") or None,
                  clock_file=Path(env.get("SHAKEDOWN_CLOCK_FILE", "/shakedown/clock.json")),
                  contracts_dir=Path(env.get("TRAFFIC_CONTRACTS", "/app/contracts")),
                  port=int(env.get("TRAFFIC_PORT", "9100")))
