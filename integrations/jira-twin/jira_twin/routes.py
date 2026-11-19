"""The twin's router: the REST operations, and Jira's 404 for any other path."""
from arena_part.twin import Response, Router

from . import rest
from .rest.api import not_found


class JiraRouter(Router):
    def not_found(self, req) -> Response:
        return not_found(req)


def build() -> JiraRouter:
    router = JiraRouter()
    rest.register(router)
    return router
