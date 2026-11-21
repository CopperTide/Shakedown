import httpx

from conftest import POD_HOST, make_ops, route, vllm
from gpupod.smoke import smoke

BASE = f"https://{POD_HOST}"


def test_smoke_against_a_fake_vllm(tmp_path, runner, clock):
    ops = make_ops(tmp_path, runner, clock, http=route({POD_HOST: vllm("k1")}))
    report = smoke(ops, BASE, "k1", "openai/gpt-oss-120b")
    assert report.ok, report.lines()
    names = [c.name for c in report.checks]
    assert names == ["health", "models with the key", "models without the key", "tool call with reasoning",
                     "answer after the tool result", "decode speed"]
    assert report.decode_tok_s and report.decode_tok_s > 0


def test_smoke_fails_with_a_clear_line_when_reasoning_is_missing(tmp_path, runner, clock):
    ops = make_ops(tmp_path, runner, clock, http=route({POD_HOST: vllm("k1", reasoning=False)}))
    report = smoke(ops, BASE, "k1", "openai/gpt-oss-120b")
    assert not report.ok
    failed = [c for c in report.checks if not c.ok]
    assert [c.name for c in failed] == ["tool call with reasoning"]
    assert "no message.reasoning" in failed[0].detail
    assert any(line.startswith("FAIL tool call with reasoning") for line in report.lines())


def test_smoke_with_a_wrong_key_fails_the_key_check(tmp_path, runner, clock):
    ops = make_ops(tmp_path, runner, clock, http=route({POD_HOST: vllm("k1")}))
    report = smoke(ops, BASE, "wrong", "openai/gpt-oss-120b")
    assert [c.name for c in report.checks if not c.ok][0] == "models with the key"


class _Branch:
    def __init__(self, events):
        self._events, self.stopped, self.run_id = events, False, "r1"

    def register(self, agent, **kw):
        self.registered = (agent, kw)
        return type("Ep", (), {"gateway_url": "https://shaked.test/gateway/b1/smoke/v1"})()

    def record(self):
        return type("Rec", (), {"events": self._events})()

    def stop(self):
        self.stopped = True


class _Arena:
    def __init__(self, branch):
        self.branch, self.deleted = branch, []

    def tasks(self):
        return [type("T", (), {"id": "t1"})()]

    def external_run(self, task):
        return self.branch

    def delete_run(self, run_id):
        self.deleted.append(run_id)


def _event(kind, payload):
    return type("E", (), {"kind": kind, "payload": payload})()


def test_smoke_through_shaked_finds_the_gateway_reasoning(tmp_path, runner, clock):
    branch = _Branch([_event("model_call", {}), _event("reasoning", {"source": "gateway", "model_call_seq": 3})])
    arena = _Arena(branch)
    hosts = {POD_HOST: vllm("k1"), "shaked.test": lambda r: httpx.Response(200, json={"ok": True})}
    ops = make_ops(tmp_path, runner, clock, http=route(hosts))
    report = smoke(ops, BASE, "k1", "openai/gpt-oss-120b", through_shaked="http://127.0.0.1:4510",
                   arena_factory=lambda url: arena)
    assert report.ok, report.lines()
    assert report.checks[-1].name == "reasoning recorded through shaked"
    assert branch.registered == ("smoke", {"kind": "external", "provider": "local:runpod",
                                           "model": "openai/gpt-oss-120b"})
    assert branch.stopped and arena.deleted == ["r1"]


def test_smoke_through_shaked_without_reasoning_fails(tmp_path, runner, clock):
    branch = _Branch([_event("model_call", {})])
    hosts = {POD_HOST: vllm("k1"), "shaked.test": lambda r: httpx.Response(200, json={"ok": True})}
    ops = make_ops(tmp_path, runner, clock, http=route(hosts))
    report = smoke(ops, BASE, "k1", "openai/gpt-oss-120b", through_shaked="http://x",
                   arena_factory=lambda url: _Arena(branch))
    assert not report.ok and branch.stopped
