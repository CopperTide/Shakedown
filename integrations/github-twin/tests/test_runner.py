import os

from github_twin.actions.runner import Limits, RealDocker, build_argv, job_argv
from github_twin.actions.workflow import Job, Step

STEP = Step("Tests", run="pytest -q")
JOB = Job("test", "test", "ubuntu-latest", "base:1", (), None, {}, (STEP,))
LIMITS = Limits(memory="384m", cpus="1", pids=256, user="1000:1000")


def test_the_job_command_line(tmp_path):
    assert job_argv(JOB, STEP, tmp_path, {"CI": "true"}, LIMITS, "job-7-1") == [
        "docker", "run", "--rm", "--pull", "never", "--network", "none",
        "--memory", "384m", "--memory-swap", "384m", "--cpus", "1", "--pids-limit", "256", "--user", "1000:1000",
        "--name", "job-7-1", "--label", "shakedown.github-twin=job",
        "-v", f"{tmp_path}:/github/workspace", "-w", "/github/workspace", "-e", "CI=true",
        "base:1", "sh", "-euc", "pytest -q"]


def test_the_build_command_line(tmp_path):
    assert build_argv(tmp_path, "services/orders", "shakedown", "orders", "abc") == [
        "docker", "build", "--pull=false", "--network", "none", "-t", "shakedown/orders:abc",
        str(tmp_path / "services" / "orders")]


def test_a_build_context_stays_in_the_checkout(tmp_path):
    assert build_argv(tmp_path, "../..", "p", "x", "abc") is None


def _fake_binary(tmp_path, body: str):
    log = tmp_path / "calls.log"
    exe = tmp_path / "docker"
    exe.write_text(f"#!/bin/sh\necho \"$@\" >> {log}\n{body}\n")
    exe.chmod(0o755)
    return str(exe), log


def test_real_docker_gives_the_exit_code_and_output(tmp_path):
    exe, _ = _fake_binary(tmp_path, 'echo out; echo err >&2; exit 3')
    done = RealDocker(exe).run(["docker", "run", "x"], timeout_s=10)
    assert (done.code, done.output) == (3, "out\nerr\n")


def test_a_step_that_runs_too_long_is_removed(tmp_path):
    exe, log = _fake_binary(tmp_path, 'case "$1" in run) sleep 30;; esac')
    done = RealDocker(exe, poll_s=0.05).run(["docker", "run", "--name", "job-9-1", "img"], timeout_s=0.3)
    assert done.code == 124 and done.output.endswith("the step timed out after 0.3 s\n")
    assert "rm -f job-9-1" in log.read_text()


def test_a_cancelled_step_is_removed(tmp_path):
    exe, log = _fake_binary(tmp_path, 'case "$1" in run) sleep 30;; esac')
    done = RealDocker(exe, poll_s=0.05).run(["docker", "run", "--name", "job-9-2", "img"], timeout_s=30,
                                            cancelled=lambda: True)
    assert done.code == 137 and "rm -f job-9-2" in log.read_text()


def test_long_output_keeps_its_tail(tmp_path):
    exe, _ = _fake_binary(tmp_path, 'head -c 300000 /dev/zero | tr "\\0" a; echo; echo last')
    done = RealDocker(exe, max_output=1000).run(["docker", "run", "x"], timeout_s=10)
    assert len(done.output) <= 1100 and done.output.endswith("last\n") and "output cut" in done.output


def test_limits_default_to_the_twins_user():
    assert Limits.from_config({}).user == f"{os.getuid()}:{os.getgid()}"
    assert Limits.from_config({"job_memory": "256m"}).memory == "256m"
