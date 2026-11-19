import json
import tomllib

import pytest
from support import call, call_error, copy_with

from worldbench import needs
from worldbench.builder import Builder


def test_preview_answers_what_review_shows(builder):
    p = call(builder, "preview", {"id": "shop", "choices": {"company": {"name": "Acme Tools"}}})
    assert (p["name"], p["title"], p["domain"]) == ("acme-tools", "Acme Tools", "acme-tools.test")
    assert p["problems"] == [] and p["needs"]["memory_mb"] == 90


def test_preview_lists_parts_hosts_people_events_and_tasks(builder):
    p = call(builder, "preview", {"id": "shop", "choices": {"size": 4, "systems": ["gamma"]}})
    assert p["parts"] == [{"part": "alpha", "integration": "toy-a", "system": "alpha"},
                          {"part": "beta", "integration": "toy-b", "system": "beta"},
                          {"part": "gamma", "integration": "toy-g", "system": "gamma"}]
    assert p["hosts"] == ["alpha.example-co.test"] and len(p["people"]) == 4
    assert p["events"] == [{"id": "slow-supplier", "label": "A supplier is slow", "kind": "trap", "on": False},
                           {"id": "rush", "label": "A rush order", "kind": "event", "on": True}]
    assert p["tasks"] == [{"id": "alpha-task", "reason": "Ship order 1 for Example Co", "checks": 1},
                          {"id": "beta-task", "reason": "Staff the Example Co counter", "checks": 0}]
    assert p["needs"] == {"images": [{"image": "toy:1", "present": True, "hint": "make toy"}], "memory_mb": 290}
    assert p["dropped"] == []


def test_a_preview_with_problems_still_names_the_world(builder_without_docker):
    p = call(builder_without_docker, "preview", {"id": "shop", "choices": {"systems": ["gamma"], "size": 99}})
    assert p["name"] == "example-co" and len(p["people"]) == 9 and p["tasks"] == []
    assert p["problems"] == [{"path": "size", "message": "Size is between 2 and 40"},
                             {"path": "needs.images.toy:1", "message": "the Docker image toy:1 is missing: make toy"}]


def test_generate_writes_a_package_arenad_can_load(builder, tmp_path):
    out = call(builder, "generate", {"id": "shop", "choices": {}, "dir": str(tmp_path / "acme")})
    toml = tomllib.loads((tmp_path / "acme" / "arena-package.toml").read_text())
    assert toml["kind"] == "benchmark" and toml["opt_in"] is True and toml["world"]["template"] == "shop"
    assert toml["version"] == out["version"] and out["version"].startswith("1.0.0+")


def test_the_package_names_the_world_and_runs_the_world_benchmark(builder, tmp_path):
    out = call(builder, "generate", {"id": "shop", "choices": {"company": {"name": "Ærø \"Q\" Co"}},
                                     "dir": str(tmp_path / "w")})
    toml = tomllib.loads((tmp_path / "w" / "arena-package.toml").read_text())
    assert out == {"name": "aero-q-co", "title": "Ærø \"Q\" Co", "version": toml["version"]}
    assert (toml["name"], toml["protocol"], toml["world"]["title"]) == ("aero-q-co", 1, "Ærø \"Q\" Co")
    assert toml["world"]["template_name"] == "Shop" and toml["needs"] == {"gpu_vram_gb": 0, "services": []}
    assert toml["run"]["command"] == ["uv", "run", "--project", "${ARENA_DIR}/benchmarks/world", "python", "-m",
                                      "worldbench", "--world", "world.json", "--version", toml["version"]]
    world = json.loads((tmp_path / "w" / "world.json").read_text())
    assert world["name"] == "aero-q-co" and (tmp_path / "w" / "files" / "seed" / "readme.txt").is_file()


def test_the_same_choices_give_the_same_version(builder, tmp_path):
    a = call(builder, "generate", {"id": "shop", "choices": {"seed": 3}, "dir": str(tmp_path / "a")})
    b = call(builder, "generate", {"id": "shop", "choices": {"seed": 3}, "dir": str(tmp_path / "b")})
    c = call(builder, "generate", {"id": "shop", "choices": {"seed": 4}, "dir": str(tmp_path / "c")})
    assert a["version"] == b["version"] != c["version"]


def test_generate_refuses_a_missing_image(builder_without_docker, tmp_path):
    e = call_error(builder_without_docker, "generate", {"id": "shop", "choices": {"systems": ["alpha", "gamma"]}, "dir": str(tmp_path / "x")})
    assert e["code"] == -32602 and e["data"]["problems"][0]["path"] == "needs.images.toy:1"
    assert not (tmp_path / "x").exists()


def test_generate_refuses_problems_with_the_first_in_its_message(builder, tmp_path):
    e = call_error(builder, "generate", {"id": "shop", "choices": {"size": 99}, "dir": str(tmp_path / "x")})
    assert e["message"] == "the world can't be built: size: Size is between 2 and 40"
    assert e["data"]["problems"] == [{"path": "size", "message": "Size is between 2 and 40"}]
    assert not (tmp_path / "x").exists()


def test_generate_into_an_existing_dir_is_refused(builder, tmp_path):
    (tmp_path / "x").mkdir()
    e = call_error(builder, "generate", {"id": "shop", "choices": {}, "dir": str(tmp_path / "x")})
    assert (e["code"], e["message"]) == (-32602, f"{tmp_path / 'x'} exists already")
    assert list((tmp_path / "x").iterdir()) == []


def test_templates_lists_every_root(fixtures, tmp_path, monkeypatch):
    copy_with(fixtures / "shop", tmp_path, {"id": "other-shop", "name": "Other shop"})
    monkeypatch.setenv("ARENA_TEMPLATES", f"{tmp_path}::{tmp_path / 'missing'}")
    listed = call(Builder.from_env(), "templates", {})
    other = next(t for t in listed if t["id"] == "other-shop")
    assert other == {"id": "other-shop", "name": "Other shop", "summary": "A neutral fixture template",
                     "version": "1.0.0", "systems": ["Alpha", "Beta", "Gamma"]}
    assert Builder.from_env().roots[0].name == "templates"


def test_template_answers_its_form_and_an_unknown_id_is_refused(builder):
    assert call(builder, "template", {"id": "shop"})["size"]["default"] == 10
    assert call_error(builder, "template", {"id": "nope"}) == {"code": -32602, "message": "no template nope"}
    assert call_error(builder, "preview", {"id": "shop"})["message"] == "invalid params: missing choices"


def test_an_image_is_present_only_when_docker_says_so(monkeypatch, tmp_path):
    monkeypatch.setenv("PATH", str(tmp_path))
    assert needs.image_present("toy:1") is False
    fake = tmp_path / "docker"
    fake.write_text("#!/bin/sh\n[ \"$4\" = \"{{.Id}}\" ] && [ \"$5\" = \"toy:1\" ]\n")
    fake.chmod(0o755)
    assert needs.image_present("toy:1") is True and needs.image_present("toy:2") is False


def test_the_program_answers_templates_over_the_line_protocol(fixtures, tmp_path):
    import os
    import subprocess
    import sys
    env = {**os.environ, "ARENA_TEMPLATES": str(fixtures)}
    proc = subprocess.run([sys.executable, "-m", "worldbench.builder"], cwd=tmp_path, env=env, capture_output=True,
                          text=True, input='{"jsonrpc":"2.0","id":1,"method":"templates"}\n', timeout=30)
    ids = [t["id"] for t in json.loads(proc.stdout.splitlines()[0])["result"]]
    assert ids == ["bad-include", "bad-placeholder", "shop"]


def test_the_program_adds_the_template_roots_its_command_names(fixtures, tmp_path):
    import os
    import subprocess
    import sys
    env = {k: v for k, v in os.environ.items() if k != "ARENA_TEMPLATES"}
    proc = subprocess.run([sys.executable, "-m", "worldbench.builder", "--templates", str(fixtures)], cwd=tmp_path,
                          env=env, capture_output=True, text=True,
                          input='{"jsonrpc":"2.0","id":1,"method":"templates"}\n', timeout=30)
    ids = [t["id"] for t in json.loads(proc.stdout.splitlines()[0])["result"]]
    assert ids == ["bad-include", "bad-placeholder", "shop"]


def test_a_preview_with_problems_still_lists_the_people_it_drew(builder):
    blank = call(builder, "preview", {"id": "shop", "choices": {"people": {"edits": {"clerk-002": {"name": " "}}}}})
    assert blank["problems"] == [{"path": "people.edits.clerk-002.name", "message": "a person needs a name"}]
    assert [p["id"] for p in blank["people"]][:3] == ["clerk-001", "clerk-002", "clerk-003"]
    assert blank["tasks"] == [] and blank["events"] == []
    low = call(builder, "preview", {"id": "shop", "choices": {"people": {"counts": {"clerk": 0}}}})
    assert low["problems"][0]["path"] == "people.counts.clerk" and len(low["people"]) == 9



def test_a_company_name_beyond_ascii_is_kept_in_the_package_manifest(builder, tmp_path):
    name = "Café 🚀 \"Tools\"\x7f"
    call(builder, "generate", {"id": "shop", "choices": {"company": {"name": name}}, "dir": str(tmp_path / "w")})
    toml = tomllib.loads((tmp_path / "w" / "arena-package.toml").read_text(encoding="utf-8"))
    assert toml["world"]["title"] == name


@pytest.mark.parametrize("choices, path", [
    ({"options": {"alpha": 5}}, "options.alpha"),
    ({"people": {"counts": [1]}}, "people.counts"),
    ({"people": {"added": 5}}, "people.added"),
    ({"people": {"added": [{"name": "Al", "role": ["clerk"]}]}}, "people.added[0].role"),
    ({"people": {"removed": 5}}, "people.removed"),
    ({"people": {"removed": [{"id": 1}]}}, "people.removed[0]"),
    ({"people": {"edits": [1]}}, "people.edits"),
    ({"systems": [{"id": "alpha"}]}, "systems[0]"),
    ({"events": {"rush": [1]}}, "events.rush"),
    ({"events": {"rush": {"params": [1]}}}, "events.rush.params"),
])
def test_malformed_choices_are_problems_not_errors(builder, choices, path):
    p = call(builder, "preview", {"id": "shop", "choices": choices})
    assert path in [x["path"] for x in p["problems"]], p["problems"]
