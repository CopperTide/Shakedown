"""A world's own data (docs/protocol.md, "World data"): the files a project world marks "Every run starts from
this", which shaked gives every part in its `start` config, and the benchmark with `gold_actions` and `score`, under
`world_data`. Absent when the world has none: the part then starts from what it carries itself.

    data = worlddata.of(config)
    db = data.read_json("db.json") if data and data.find("db.json") else packaged_db()

In a benchmark, `self.world_data()` is the data of the `gold_actions` or `score` call in progress. Standard library
only."""
import json
from dataclasses import dataclass, field
from pathlib import Path

#: The key that holds the data in a part's config and in `gold_actions` and `score` params.
KEY = "world_data"


@dataclass(frozen=True)
class DataFile:
    """One file of the data, at its path in the world's folder (e.g. `data/db.json`)."""
    path: str
    seeds: str | None = None
    size: int = 0
    sha256: str = ""


@dataclass(frozen=True)
class WorldData:
    """The data a part or a benchmark call was given: `digest` changes whenever any file does (it is part of the
    part's fingerprint); `dir` holds the files at their paths and never changes."""
    digest: str
    dir: Path
    world: str = ""
    files: tuple = field(default_factory=tuple)

    def find(self, name: str) -> DataFile | None:
        """The file named `name`: by its path, else the one file whose last segment it is."""
        for f in self.files:
            if f.path == name:
                return f
        named = [f for f in self.files if f.path.rsplit("/", 1)[-1] == name]
        return named[0] if len(named) == 1 else None

    def path(self, name: str) -> Path | None:
        """Where the file named `name` is on disk, or None."""
        f = self.find(name)
        return self.dir / f.path if f else None

    def read_text(self, name: str) -> str:
        p = self.path(name)
        if p is None:
            raise FileNotFoundError(f"the world's data has no {name}")
        return p.read_text(encoding="utf-8")

    def read_json(self, name: str):
        return json.loads(self.read_text(name))


def of(value) -> WorldData | None:
    """The data in `value` (a part's config, or a call's params), if it has any."""
    d = value.get(KEY) if isinstance(value, dict) else None
    if not isinstance(d, dict) or not d.get("digest") or not d.get("dir"):
        return None
    files = tuple(DataFile(path=f["path"], seeds=f.get("seeds"), size=int(f.get("size") or 0),
                           sha256=f.get("sha256") or "") for f in d.get("files", []) if isinstance(f, dict) and f.get("path"))
    return WorldData(digest=d["digest"], dir=Path(d["dir"]), world=d.get("world") or "", files=files)
