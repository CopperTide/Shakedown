"""DocsPart: a part read as `{key: doc}` for its instances and saves — inspect, diff, the save folders."""
import json

import pytest

from shakedown_part import PartError, saves
from shakedown_part.docspart import DocsPart


class Notes(DocsPart):
    NAME = "notes"

    def start(self, instance, config, seed, clock_ms, from_=None):
        self.instances[instance] = dict(config)

    def live_docs(self, instance):
        return {f"note/{k}": v for k, v in self._instance(instance).items()}

    def saved_docs(self, save_dir):
        try:
            return json.loads((save_dir / "docs.json").read_text())
        except OSError:
            raise PartError(-32001, f"unknown save {save_dir.name}") from None

    def save(self, instance, save, dir):
        (dir / "docs.json").write_text(json.dumps(self.live_docs(instance)))
        return {"bytes": 1, "exact": True}


class Loose(Notes):
    def same(self, a, b):
        return a == b or (isinstance(a, float) and isinstance(b, float) and abs(a - b) < 0.1)


def started(part=None, **notes):
    part = part or Notes()
    part.start("i1", notes, 0, 0)
    return part


def test_inspect_gives_the_asked_keys_or_all_of_them():
    part = started(a=1, b=2)
    assert part.inspect({"instance": "i1"}, []) == {"docs": {"note/a": 1, "note/b": 2}}
    assert part.inspect({"instance": "i1"}, ["note/b", "note/z"]) == {"docs": {"note/b": 2, "note/z": None}}


def test_a_save_is_read_from_its_folder_under_the_parts_name(saves_root):
    part = started(a=1)
    folder = saves.root("notes") / "s1"
    folder.mkdir(parents=True)
    part.save("i1", "s1", folder)
    assert part.inspect({"save": "s1"}, []) == {"docs": {"note/a": 1}}
    part.drop_save("s1")
    with pytest.raises(PartError, match="unknown save s1"):
        part.inspect({"save": "s1"}, [])


def test_unknown_instances_and_targets_are_refused():
    part = started(a=1)
    with pytest.raises(PartError, match="unknown instance i9"):
        part.inspect({"instance": "i9"}, [])
    with pytest.raises(PartError, match="invalid params: target needs instance or save"):
        part.diff({"instance": "i1"}, {})


def test_diff_names_the_keys_that_differ_by_the_parts_own_sameness():
    part = started(a=1.0, b=2)
    part.start("i2", {"a": 1.05, "c": 3}, 0, 0)
    assert part.diff({"instance": "i1"}, {"instance": "i2"}) == {"differing": ["note/a", "note/b", "note/c"]}
    loose = started(Loose(), a=1.0, b=2)
    loose.start("i2", {"a": 1.05, "b": 2}, 0, 0)
    assert loose.diff({"instance": "i1"}, {"instance": "i2"}) == {"differing": []}


def test_stop_usage_and_gc():
    part = started(a=1)
    part.stop("i1")
    part.stop("i1")
    assert part.instances == {}
    assert part.usage() == {"bytes": 0}
    assert part.gc([]) == saves.gc("notes", [])
