"""What the plant and the mirror share: the kit's DocsPart over the factory's documents, whose saves keep them
in `views.DOCS_FILE` and whose physics floats need only be close."""
from arena_part import DocsPart as KitDocsPart

from . import views


class DocsPart(KitDocsPart):
    NAME = ""  # the package name; saves are at $ARENA_SAVES/<NAME>/<save>/

    def saved_docs(self, save_dir) -> dict:
        return views.read_docs(save_dir)

    def same(self, a, b) -> bool:
        return views.same(a, b)
