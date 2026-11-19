"""The REST API under `/api/v3`, every operation listed in `covered.COVERED`."""
from . import commits, contents, git, meta, repos
from .api import Api

MODULES = (meta, repos, contents, commits, git)


def register(router) -> None:
    api = Api(router)
    for module in MODULES:
        module.register(api)
