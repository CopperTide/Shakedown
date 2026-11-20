"""``arena_part``, the Python kit's name until 2026-11-19: now :mod:`shakedown_part`. ``import arena_part``
and ``from arena_part.<module> import ...`` give shakedown_part's own modules and objects, so parts written
for the old name keep working. Importing it warns once. New code: ``import shakedown_part``.
"""

import importlib
import importlib.abc
import importlib.util
import sys
import warnings

from shakedown_part import *  # noqa: F401,F403
from shakedown_part import PROTOCOL, __all__  # noqa: F401

warnings.warn(
    "the `arena_part` kit is now `shakedown_part`: use `import shakedown_part` "
    "(`arena_part` is an alias kept for old parts)",
    FutureWarning,
    stacklevel=2,
)


class _OldName(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    """Finds ``arena_part.<x>`` and loads it as the module ``shakedown_part.<x>`` itself (not a copy)."""

    def find_spec(self, name, path=None, target=None):
        if name.startswith(__name__ + "."):
            return importlib.util.spec_from_loader(name, self)
        return None

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        # The import system hands back what ends up in sys.modules under the name.
        sys.modules[module.__name__] = importlib.import_module("shakedown_part" + module.__name__[len(__name__):])


sys.meta_path.insert(0, _OldName())

# As before, `import arena_part` also gives the submodules loaded with it.
for _name in [n for n in sys.modules if n.startswith("shakedown_part.")]:
    sys.modules.setdefault(__name__ + _name[len("shakedown_part"):], sys.modules[_name])
    if _name.count(".") == 1:
        globals().setdefault(_name.split(".", 1)[1], sys.modules[_name])
