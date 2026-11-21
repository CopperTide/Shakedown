"""What `/version` answers: the commit the running image was built from and the installed versions of the
service's pinned requirements (each with its major version as a number, so checks can compare it)."""
import os
import re
from importlib import metadata
from pathlib import Path

_PIN = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)(?:\[[^\]]*\])?==")


def image_sha(env=os.environ) -> str:
    """`APP_SHA` when set, else the tag of `APP_IMAGE` (CI tags each image with its commit), else `dev`."""
    if env.get("APP_SHA"):
        return env["APP_SHA"]
    image = env.get("APP_IMAGE", "")
    name = image.rsplit("/", 1)[-1]
    return name.split(":", 1)[1] if ":" in name else "dev"


def pinned(requirements: Path) -> list[str]:
    lines = (line.split("#", 1)[0].strip() for line in requirements.read_text(encoding="utf-8").splitlines())
    return [m.group(1) for m in map(_PIN.match, lines) if m]


def version_doc(requirements: Path, env=os.environ) -> dict:
    deps = {}
    for name in pinned(requirements):
        installed = metadata.version(name)
        key = re.sub(r"[-.]+", "_", name.lower())
        deps[key] = installed
        deps[f"{key}_major"] = int(installed.split(".", 1)[0])
    return {"sha": image_sha(env), "deps": deps}
