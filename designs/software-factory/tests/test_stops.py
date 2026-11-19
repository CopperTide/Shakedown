import httpx
import openai
import pytest

from arena.adapters import langgraph
from fake_model import FakeModel, FakeRun, say, status
from factory_designs.stops import run_design, stopped_reason


def _status_error(code: int) -> openai.APIStatusError:
    req = httpx.Request("POST", "http://127.0.0.1/gateway/b1/a/v1/chat/completions")
    resp = httpx.Response(code, request=req, json={"error": "x"})
    return openai.APIStatusError(f"Error code: {code}", response=resp, body={"error": "x"})


def test_stopped_reason_knows_the_limit_and_the_end():
    req = httpx.Request("POST", "http://127.0.0.1/v1/chat/completions")
    assert stopped_reason(_status_error(402)) == "the run's spend limit was reached"
    assert stopped_reason(_status_error(409)) == "the run ended"
    assert stopped_reason(_status_error(404)) == "the run ended"
    assert stopped_reason(_status_error(524)).startswith("the model kept failing (524)")
    assert stopped_reason(openai.APIConnectionError(request=req)).startswith(
        "the model could not be reached")
    assert stopped_reason(ValueError("x")) is None
    assert stopped_reason(_status_error(400)) is None


def test_a_wrapped_error_is_recognised():
    try:
        try:
            raise _status_error(402)
        except openai.APIStatusError as e:
            raise RuntimeError("the graph failed") from e
    except RuntimeError as wrapped:
        assert stopped_reason(wrapped) == "the run's spend limit was reached"


def _open(run):
    from contextlib import contextmanager

    @contextmanager
    def opener():
        yield run

    return opener


def test_repeated_upstream_failure_ends_the_design_cleanly():
    html = "<html><body>A timeout occurred</body></html>"
    with FakeModel({"a": [status(524, html)] * 4 + [say("late")]}) as fake:
        run = FakeRun(fake)

        def main(r):
            a = r.agent("a")
            langgraph.chat_model(a, "openai/gpt-oss-120b").invoke("hello")

        run_design(main, open_run=_open(run))
        assert len(fake.requests) >= 3
        steps = [(a, t) for a, k, t in run.reasoning if k == "step"]
        assert len(steps) == 1 and steps[0][1].startswith("stopped: the model kept failing (524)")


def test_other_errors_go_up():
    with FakeModel({}) as fake:
        run = FakeRun(fake)

        def main(r):
            r.agent("a")
            raise ValueError("a bug")

        with pytest.raises(ValueError, match="a bug"):
            run_design(main, open_run=_open(run))
        assert run.reasoning == []
