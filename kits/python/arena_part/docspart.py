"""A part read as `{key: doc}`: its instances live (`live_docs`) and its saves from their folders
(`saved_docs`), with `inspect`, `diff`, the save folders under `$ARENA_SAVES/<NAME>/` and the instance table
every part keeps.

Subclass `DocsPart` and give it `NAME` (the package name, which names its save folders), `live_docs(instance)`
and `saved_docs(save_dir)`; override `same(a, b)` when two documents may differ and still be the same (floats
within a tolerance)."""
from pathlib import Path

from . import errors, saves
from .serve import Part


def pick(docs: dict, keys) -> dict:
    """inspect's answer: the asked keys (null when absent), or every key when none are asked."""
    return {k: docs.get(k) for k in keys} if keys else docs


class DocsPart(Part):
    NAME = ""

    def __init__(self):
        self.instances: dict = {}

    def live_docs(self, instance) -> dict:
        raise NotImplementedError("live_docs")

    def saved_docs(self, save_dir: Path) -> dict:
        raise NotImplementedError("saved_docs")

    def same(self, a, b) -> bool:
        return a == b

    def _instance(self, instance):
        try:
            return self.instances[instance]
        except KeyError:
            raise errors.unknown(f"unknown instance {instance}") from None

    def docs_of(self, target) -> dict:
        if "instance" in target:
            return self.live_docs(target["instance"])
        if "save" in target:
            return self.saved_docs(saves.root(self.NAME) / target["save"])
        raise errors.invalid("invalid params: target needs instance or save")

    def inspect(self, target, keys):
        return {"docs": pick(self.docs_of(target), keys)}

    def diff(self, a, b):
        da, db = self.docs_of(a), self.docs_of(b)
        return {"differing": sorted(k for k in da.keys() | db.keys() if not self.same(da.get(k), db.get(k)))}

    def drop_save(self, save):
        saves.drop(self.NAME, save)

    def stop(self, instance):
        self.instances.pop(instance, None)

    def usage(self):
        return {"bytes": 0}  # every save lives under ARENA_SAVES, which arenad measures itself

    def gc(self, keep):
        return saves.gc(self.NAME, keep)
