"""How a design's run ends when the world stops it: the spend limit (the gateway answers 402),
the run's end (404 or 409 once its window or a stop ended it), or a model that keeps failing
after the client's retries. Those end the design quietly, with a ``stopped:`` step recorded, and
the run is scored at its window like any other. Anything else is a bug and goes up."""

from __future__ import annotations

import os
from contextlib import ExitStack
from typing import Any, Callable, ContextManager, Iterator, Optional

import openai

from arena import driver
from arena.errors import ArenaError, Conflict, NotFound

from .bridge import ProxyBridge, bridge_socket

#: Statuses the OpenAI client retries; seeing one means its retries ran out.
RETRIED = {408, 429}


def _chain(exc: BaseException) -> Iterator[BaseException]:
    """``exc`` and what it wraps (causes, contexts and framework-held originals), once each."""
    seen: set[int] = set()
    todo = [exc]
    while todo:
        e = todo.pop(0)
        if id(e) in seen:
            continue
        seen.add(id(e))
        yield e
        todo += [x for x in (e.__cause__, e.__context__, getattr(e, "original_exception", None))
                 if isinstance(x, BaseException)]


def _reason(e: BaseException) -> Optional[str]:
    if isinstance(e, (Conflict, NotFound)):  # shaked itself: the run's clock or record is gone
        return "the run ended"
    if isinstance(e, openai.APIStatusError):
        code = e.status_code
        if code == 402:
            return "the run's spend limit was reached"
        if code in (404, 409):
            return "the run ended"
        if code >= 500 or code in RETRIED:
            return f"the model kept failing ({code}) after the client's retries"
        return None
    if isinstance(e, openai.APIConnectionError):  # includes APITimeoutError
        return f"the model could not be reached after the client's retries: {e}"
    return None


def stopped_reason(exc: BaseException) -> Optional[str]:
    """Why the world stopped this design, or None when ``exc`` is not such a stop."""
    return next((r for r in map(_reason, _chain(exc)) if r), None)


def _report(run: Any, reason: str) -> None:
    for a in run.agents:
        try:
            a.report_reasoning(f"stopped: {reason}", kind="step")
        except ArenaError:
            pass  # the run already ended: nothing more to record


def _proxy_url() -> Optional[str]:
    return os.environ.get("ARENA_PROXY") or os.environ.get("HTTPS_PROXY")


def run_design(main: Callable[[Any], None],
               open_run: Callable[[], ContextManager[Any]] = driver.run) -> None:
    """Runs ``main(run)`` in a driver run, with the proxy bridge open when the world serves
    HTTP. A stop from the world ends it quietly (see the module); other errors go up."""
    with open_run() as run, ExitStack() as stack:
        url = _proxy_url()
        if url:
            stack.enter_context(ProxyBridge(url, bridge_socket()))
        try:
            main(run)
        except Exception as exc:  # noqa: BLE001 - sorted below
            reason = stopped_reason(exc)
            if reason is None:
                raise
            _report(run, reason)
