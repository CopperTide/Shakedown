"""The REST API: platform (`/rest/api/{2,3,latest}`) and agile (`/rest/agile/1.0`) operations."""
from . import agile, issues, platform, search
from .api import Api


def register(router) -> None:
    api = Api(router)
    for module in (platform, issues, search, agile):
        module.register(api)
