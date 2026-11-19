"""Readers the software-factory file tests share: YAML, the platform's compose file under a profile, and the
metric names a PromQL expression reads."""
import re
from pathlib import Path

import yaml

MIB = {"k": 1 / 1024, "m": 1, "g": 1024}


def load_yaml(path: Path):
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def profile_env(platform: Path, profile: str) -> dict:
    """The environment the `observability` system's `profile` option selects (string values, as compose reads)."""
    return {k: str(v) for k, v in load_yaml(platform / "profiles" / f"{profile}.yaml").items()}


def images_env(sf_dir: Path) -> dict:
    """`IMAGE_<SERVICE>` for every compose service the template's images.yaml names."""
    images = load_yaml(sf_dir / "images.yaml")["images"]
    return {f"IMAGE_{svc.upper().replace('-', '_')}": i["image"] for i in images for svc in i["services"]}


def services(platform: Path, profile: str) -> dict:
    """The compose services that run under `profile` (`full` adds those under `profiles: [full]`)."""
    all_services = load_yaml(platform / "compose.yaml")["services"]
    return {name: s for name, s in all_services.items() if profile == "full" or not s.get("profiles")}


def mib(value: str) -> int:
    m = re.fullmatch(r"(\d+)([kmg])", value.strip().lower())
    assert m, f"a memory size like 96m, not {value!r}"
    return round(int(m.group(1)) * MIB[m.group(2)])


def interpolate(value: str, env: dict) -> str:
    """Compose's `${NAME}`, `${NAME:-default}` and `${NAME:?message}` over `env`."""
    def one(m):
        name, op, rest = m.group(1), m.group(2), m.group(3)
        if name in env and (env[name] or op != ":-"):
            return env[name]
        assert op != ":?", f"{name} is required: {rest}"
        return rest if op == ":-" else ""
    return re.sub(r"\$\{([A-Z0-9_]+)(?:(:-|:\?)([^}]*))?\}", one, value)


def compose_limits(platform: Path, profile: str) -> dict:
    """Each service's `mem_limit` in MiB under `profile`, its `${MEM_…}` filled from that profile's env."""
    env = profile_env(platform, profile)
    return {name: mib(interpolate(str(s["mem_limit"]), env)) for name, s in services(platform, profile).items()}


_STRINGS = re.compile(r'"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)*\'')
_MATCHERS = re.compile(r"\{[^}]*\}")
_GROUPING = re.compile(r"\b(?:by|without|on|ignoring|group_left|group_right)\s*\([^)]*\)")
_RANGES = re.compile(r"\[[^\]]*\]")
_NAME = re.compile(r"(?<![\w.])([a-zA-Z_:][a-zA-Z0-9_:]*)\b(?!\s*\()")
KEYWORDS = {"and", "or", "unless", "bool", "offset", "by", "without", "on", "ignoring", "group_left",
            "group_right", "inf", "nan", "le", "atan2"}


def metric_names(expr: str) -> set:
    """The metric names a PromQL expression reads: what is left once strings, label matchers, groupings, ranges,
    function calls and keywords are taken out."""
    text = _RANGES.sub(" ", _GROUPING.sub(" ", _MATCHERS.sub(" ", _STRINGS.sub(" ", expr))))
    text = re.sub(r"\$\w+", " ", text)  # Grafana variables such as $__rate_interval
    return {n for n in _NAME.findall(text) if n.lower() not in KEYWORDS}
