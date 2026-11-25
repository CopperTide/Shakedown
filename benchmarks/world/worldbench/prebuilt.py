"""The worlds shipped prebuilt: each a template built with its default choices, written as a benchmark package
`benchmarks/<template id>` named after the template. Hosted Shakedown runs these and builds none.
`python -m worldbench.prebuilt` writes them again after their template changes, keeping each one's README.md and
describe.json (which a shaked test writes, see docs/packages.md)."""
import shutil
from pathlib import Path

from .builder import PACKAGE_TEMPLATES, Builder
from .generate import generate
from .install import write_package

PREBUILT = ("software-factory",)
BENCHMARKS = Path(__file__).resolve().parents[2]
KEPT = ("describe.json", "README.md")


def write(id: str, dir: Path) -> dict:
    """Writes the prebuilt world `id` into `dir` (which must not exist): `{name, title, version}`."""
    t = Builder([PACKAGE_TEMPLATES]).find(id)
    result = generate(t, {})
    if result.problems:
        raise ValueError(f"{id} does not build with its defaults: {result.problems[0]}")
    result.world["name"] = id
    return write_package(result, t, dir)


def refresh(id: str) -> dict:
    """Writes `benchmarks/<id>` again, the files in `KEPT` kept."""
    target = BENCHMARKS / id
    tmp = BENCHMARKS / f".{id}.tmp"
    shutil.rmtree(tmp, ignore_errors=True)
    out = write(id, tmp)
    for name in KEPT:
        if (target / name).is_file():
            shutil.copy2(target / name, tmp / name)
    shutil.rmtree(target, ignore_errors=True)
    tmp.rename(target)
    return out


def main() -> None:
    for id in PREBUILT:
        print(refresh(id))


if __name__ == "__main__":
    main()
