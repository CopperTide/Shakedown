"""``arena``, the SDK's name until 2026-11-19: now :mod:`shakedown`. ``import arena`` and
``from arena.<module> import ...`` give shakedown's own modules and objects, and the old class names
(``Arena``, ``LocalArena``, ``ArenaError``, ``ArenaParallelEnv``) are the new classes. Importing it
warns once. New code: ``from shakedown import Shakedown`` (docs/sdk.md, "Old names").
"""

import importlib
import importlib.abc
import importlib.util
import sys
import warnings

from shakedown import *  # noqa: F401,F403
from shakedown import __all__ as _new_all
from shakedown import __version__  # noqa: F401
from shakedown.client import Arena
from shakedown.errors import ArenaError

warnings.warn(
    "the `arena` package is now `shakedown`: use `from shakedown import Shakedown` "
    "(`arena` and `Arena` are aliases kept for old code)",
    DeprecationWarning,
    stacklevel=2,
)


class _OldName(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    """Finds ``arena.<x>`` and loads it as the module ``shakedown.<x>`` itself (not a copy), so
    classes and errors are the same objects under both names."""

    def find_spec(self, name, path=None, target=None):
        if name.startswith(__name__ + "."):
            return importlib.util.spec_from_loader(name, self)
        return None

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        # The import system hands back what ends up in sys.modules under the name.
        sys.modules[module.__name__] = importlib.import_module("shakedown" + module.__name__[len(__name__):])


sys.meta_path.insert(0, _OldName())

# As before, `import arena` also gives the submodules loaded with it (`arena.client`, `arena.errors`, ...).
for _name in [n for n in sys.modules if n.startswith("shakedown.")]:
    sys.modules.setdefault(__name__ + _name[len("shakedown"):], sys.modules[_name])
    if _name.count(".") == 1:
        globals().setdefault(_name.split(".", 1)[1], sys.modules[_name])

__all__ = [*_new_all, "Arena", "ArenaError"]
