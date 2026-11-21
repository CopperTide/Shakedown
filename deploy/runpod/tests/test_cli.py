from pathlib import Path

from conftest import SHAKED_HOST, POD_HOST, shaked, make_ops, route, vllm
from gpupod import cli


def _main(tmp_path, runner, clock, argv, out, http=None):
    def make(args):
        ops = make_ops(tmp_path, runner, clock, http=http, out=out)
        ops.ledger.path = Path(args.ledger)
        return ops

    return cli.main(["--ledger", str(tmp_path / "ledger.jsonl"), *argv], make_ops=make)


def test_the_cli_prints_the_rate_then_refuses_past_the_cap(tmp_path, runner, clock):
    out: list[str] = []
    (tmp_path / "ledger.jsonl").write_text('{"at": "2026-11-19T20:00:00Z", "kind": "openrouter", "usd": 45.0}\n')
    code = _main(tmp_path, runner, clock, ["start", "--confirm-rate", "2.69", "--hours", "8"], out)
    assert code == 2
    assert out[0].startswith("NVIDIA H100 80GB HBM3, community: $2.69/h")
    assert "over the $50.00 budget" in out[-1]
    assert ("pod", "create") not in runner.commands()


def test_the_cli_starts_within_the_cap_and_smoke_records_the_speed(tmp_path, runner, clock):
    out: list[str] = []
    http = route({POD_HOST: vllm("unused"), SHAKED_HOST: shaked([])})
    assert _main(tmp_path, runner, clock, ["start", "--confirm-rate", "2.69", "--hours", "2"], out, http) == 0
    key = (tmp_path / "config" / "runpod.env").read_text().split("SHAKEDOWN_LOCAL_KEY_RUNPOD=")[1].split("\n")[0]
    http = route({POD_HOST: vllm(key)})
    assert _main(tmp_path, runner, clock, ["smoke"], out, http) == 0
    kinds = [line for line in (tmp_path / "ledger.jsonl").read_text().splitlines() if '"measure"' in line]
    assert kinds and '"decode_tok_s"' in kinds[0] and key not in kinds[0]


def test_the_cli_quotes_without_starting(tmp_path, runner, clock):
    out: list[str] = []
    assert _main(tmp_path, runner, clock, ["quote", "--gpu", "NVIDIA L40S", "--hours", "1"], out) == 0
    assert out[0].startswith("NVIDIA L40S, community: $0.79/h")
    assert runner.commands() == [("gpu", "list"), ("user",)]


def test_the_cli_takes_the_old_shaked_flags():
    args = cli.parser().parse_args(["--shaked", "http://127.0.0.1:9", "smoke", "--through-shaked", "http://x"])
    assert (args.shaked, args.through_shaked) == ("http://127.0.0.1:9", "http://x")
