"""The twin's router: the REST API and the Events API (`/v2/enqueue`) on one server (each host the
config gives reaches it), and PagerDuty's 404 for any other path."""
from shakedown_part.twin import Response, Router

from . import events_v2, rest
from .api import serve
from .errors import not_found


class PagerDutyRouter(Router):
    def not_found(self, req) -> Response:
        return Response(404, not_found().body())


def build() -> PagerDutyRouter:
    router = PagerDutyRouter()
    for method, path, fn in rest.ROUTES:
        router.add(method, path, lambda req, ctx, fn=fn: serve(fn, req, ctx))
    router.add("POST", "/v2/enqueue", events_v2.enqueue)
    return router
