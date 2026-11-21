import os
from pathlib import Path

from conftest import FakeDocker
from factory_designs.workbench import Completed, Workbench, container_env


def test_container_env_points_proxies_at_the_bridge_and_keeps_the_ticket():
    env = {"HTTPS_PROXY": "http://shakedown:t1@127.0.0.1:4600", "http_proxy": "http://shakedown:t1@127.0.0.1:4600/",
           "SHAKEDOWN_PROXY": "http://shakedown:t1@127.0.0.1:4600", "GH_HOST": "github.acme.test"}
    assert container_env(env) == {
        "HTTPS_PROXY": "http://shakedown:t1@127.0.0.1:3128", "http_proxy": "http://shakedown:t1@127.0.0.1:3128/",
        "SHAKEDOWN_PROXY": "http://shakedown:t1@127.0.0.1:3128", "GH_HOST": "github.acme.test"}


def test_the_container_command_line(tmp_path: Path, settings):
    ca = tmp_path / "ca.pem"
    ca.write_text("cert")
    env = {"HTTPS_PROXY": "http://shakedown:t1@127.0.0.1:4600", "SSL_CERT_FILE": str(ca)}
    docker = FakeDocker()
    wb = Workbench("coder", env, tmp_path / "ws", tmp_path / "p.sock", settings, docker)
    (tmp_path / "ws" / "coder" / "app").mkdir(parents=True)
    wb.shell("git status", cwd="app")
    ws = tmp_path / "ws" / "coder"
    assert docker.argvs == [[
        "docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
        "--tmpfs", "/tmp:size=256m", "--memory", "1g", "--pids-limit", "256", "--user", "1000:1000",
        "-v", f"{ws}:/work", "-v", f"{tmp_path / 'p.sock'}:/run/shakedown-proxy.sock", "-w", "/work/app",
        "-v", f"{ca}:{ca}:ro",
        "-e", "HTTPS_PROXY=http://shakedown:t1@127.0.0.1:3128", "-e", f"SSL_CERT_FILE={ca}",
        "shakedown-workbench:1", "timeout", "600", "sh", "-euc", "git status"]]


def test_the_workspace_is_made_and_the_root_cwd_is_work(tmp_path: Path, settings):
    docker = FakeDocker()
    wb = Workbench("a", {}, tmp_path / "ws", tmp_path / "p.sock", settings, docker)
    wb.shell("ls")
    assert (tmp_path / "ws" / "a").is_dir()
    argv = docker.argvs[0]
    assert argv[argv.index("-w") + 1] == "/work"


def test_paths_outside_the_workspace_are_refused(tmp_path: Path, bench, docker):
    (tmp_path / "secret").write_text("no")
    os.symlink(tmp_path / "secret", tmp_path / "ws" / "a" / "out")
    for p in ("../x", "/etc/passwd", "out"):
        assert bench.read_file(p) == f"error: {p} is outside your workspace"
        assert bench.write_file(p, "x") == f"error: {p} is outside your workspace"
    assert bench.shell("ls", cwd="../..") == "error: ../.. is outside your workspace"
    assert docker.argvs == []
    assert (tmp_path / "secret").read_text() == "no"


def test_files_are_read_and_written_in_the_workspace(tmp_path: Path, bench):
    assert bench.write_file("app/src/x.py", "print(1)\n").startswith("wrote ")
    assert (tmp_path / "ws" / "a" / "app" / "src" / "x.py").read_text() == "print(1)\n"
    assert bench.read_file("app/src/x.py") == "print(1)\n"
    assert bench.read_file("/work/app/src/x.py") == "print(1)\n"
    assert bench.read_file("app/none.py").startswith("error: ")


def test_a_failed_command_is_text(bench, docker):
    docker.script = [Completed(1, "HTTP 403: this agent is read-only")]
    assert bench.shell("gh pr merge 3") == "exit 1\nHTTP 403: this agent is read-only"


def test_long_output_keeps_its_tail(bench, docker, settings):
    out = "".join(f"line {i}\n" for i in range(5000))
    docker.script = [Completed(0, out)]
    text = bench.shell("make")
    assert text.startswith("exit 0\n[…cut]")
    assert text.endswith("line 4999\n")
    assert len(text) <= settings.tool_output_chars + len("exit 0\n[…cut]")


def test_run_tests_runs_pytest_in_the_folder(bench, docker):
    (bench.workspace / "app").mkdir()
    bench.run_tests("app", "-k contract")
    argv = docker.argvs[0]
    assert argv[-1] == "python -m pytest -q -k contract"
    assert argv[argv.index("-w") + 1] == "/work/app"


def test_push_gate(tmp_path: Path, settings):
    docker = FakeDocker()
    wb = Workbench("e", {}, tmp_path / "ws", tmp_path / "p.sock", settings, docker, push_gate=True)
    (wb.workspace / "app").mkdir()
    refused = "error: run the tests (run_tests) in app and see them pass before pushing"
    assert wb.shell("git push -u origin fix", cwd="app") == refused
    docker.script = [Completed(1, "1 failed")]
    wb.run_tests("app")
    assert wb.shell("git push", cwd="app") == refused
    docker.script = [Completed(0, "3 passed")]
    wb.run_tests("app")
    assert wb.shell("git push", cwd="app") == "exit 0\n"
    assert wb.shell("cd app && git push origin fix") == "exit 0\n"
    wb.write_file("app/x.py", "x = 2\n")
    assert wb.shell("git push", cwd="app") == refused
    assert wb.shell("git status", cwd="app") == "exit 0\n"
    pushes = [c for c in docker.commands if "push" in c]
    assert len(pushes) == 2


def test_without_the_gate_pushes_go_through(bench, docker):
    assert bench.shell("git push") == "exit 0\n"


def test_benches_make_one_workspace_per_agent_with_its_env(tmp_path: Path, settings, monkeypatch):
    from factory_designs.workbench import RealDocker, benches

    class Ep:
        env = {"HTTPS_PROXY": "http://shakedown:t2@127.0.0.1:4600"}

    monkeypatch.setenv("FACTORY_WORKSPACES", str(tmp_path / "ws"))
    wb = benches(settings, push_gate=True)("coder", Ep())
    assert wb.workspace == (tmp_path / "ws" / "coder").resolve()
    assert wb.env == Ep.env and wb.gate is not None and isinstance(wb.docker, RealDocker)
