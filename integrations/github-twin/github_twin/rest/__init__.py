"""The REST API under `/api/v3`, every operation listed in `covered.COVERED`."""
from . import actions, checks, commits, contents, git, hooks, issues, labels, meta, pulls, repos, reviews, search, statuses
from .api import Api

# Order matters where patterns overlap: `/commits/{ref}/status` and `/check-runs` before `/commits/{ref:path}`.
MODULES = (meta, repos, contents, statuses, checks, actions, commits, git, pulls, reviews, issues, labels, search, hooks)


def register(router) -> None:
    api = Api(router)
    for module in MODULES:
        module.register(api)
