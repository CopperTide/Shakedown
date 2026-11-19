"""Live: each observability program checks its own configuration, in the image images.yaml pins (present
locally; nothing is pulled), and promtool runs the alert rules' unit tests."""
import shutil
import subprocess

import pytest
from sf_support import load_yaml, profile_env

pytestmark = pytest.mark.live


def image(sf_dir, name: str) -> str:
    ref = next(i["image"] for i in load_yaml(sf_dir / "images.yaml")["images"] if i["id"] == name)
    if shutil.which("docker") is None or subprocess.run(["docker", "image", "inspect", ref],
                                                        capture_output=True).returncode != 0:
        pytest.skip(f"{ref} is not on this machine ({name}'s hint in images.yaml fetches it)")
    return ref


def run(*argv) -> subprocess.CompletedProcess:
    return subprocess.run(["docker", "run", "--rm", "--pull", "never", "--network", "none", *argv],
                          capture_output=True, text=True, timeout=300)


def test_live_prometheus_config_and_rules_check_and_their_unit_tests_pass(sf_dir, platform):
    ref, mount = image(sf_dir, "prometheus"), f"{platform / 'prometheus'}:/etc/prometheus:ro"
    for argv in (["check", "config", "/etc/prometheus/prometheus.yml"],
                 ["test", "rules", "/etc/prometheus/rules.test.yml"]):
        done = run("-v", mount, "-w", "/etc/prometheus", "--entrypoint", "promtool", ref, *argv)
        assert done.returncode == 0, done.stdout + done.stderr


def test_live_alertmanager_config_checks(sf_dir, platform):
    done = run("-v", f"{platform / 'alertmanager.yml'}:/etc/alertmanager/alertmanager.yml:ro",
               "--entrypoint", "amtool", image(sf_dir, "alertmanager"), "check-config",
               "/etc/alertmanager/alertmanager.yml")
    assert done.returncode == 0, done.stdout + done.stderr


def test_live_collector_config_validates(sf_dir, platform):
    done = run("-v", f"{platform / 'otel' / 'collector.yaml'}:/etc/otelcol/collector.yaml:ro",
               image(sf_dir, "otel-collector"), "validate", "--config=/etc/otelcol/collector.yaml")
    assert done.returncode == 0, done.stdout + done.stderr


@pytest.mark.parametrize("profile", ["lean", "full"])
def test_live_loki_config_verifies(sf_dir, platform, profile):
    env = profile_env(platform, profile)
    done = run("-e", f"LOKI_RETENTION={env['LOKI_RETENTION']}", "-v", f"{platform / 'loki.yaml'}:/etc/loki/loki.yaml:ro",
               image(sf_dir, "loki"), "-config.file=/etc/loki/loki.yaml", "-config.expand-env=true",
               "-verify-config=true")
    assert done.returncode == 0, done.stdout + done.stderr
