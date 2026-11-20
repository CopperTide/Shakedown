"""What stage 4's templates need of the format beyond stage 3: the world's seed and a role's field as placeholders,
placeholders in an include's path, a system that changes other parts' config when chosen (with `merge`), and a
check that needs a system."""
from pathlib import Path

import pytest
import yaml
from support import copy_with

from worldbench.expand import Context, expand
from worldbench.generate import generate
from worldbench.template import TemplateError, load_template

PEOPLE = [{"id": "clerk-001", "role": "clerk", "login": "anar"}, {"id": "clerk-002", "role": "clerk", "login": "bob"},
          {"id": "lead-001", "role": "lead", "login": "cid"}]


def ctx(**kw) -> Context:
    base = {"company": {"name": "Acme", "slug": "acme"}, "size": 3, "options": {"alpha": {"profile": "full"}},
            "people": PEOPLE, "params": {}}
    return Context(**{**base, **kw})


def test_the_seed_and_a_roles_field_are_placeholders():
    assert expand("{seed}", ctx(seed=7), Path("."), []) == 7
    assert expand("s-{seed}", ctx(seed=7), Path("."), []) == "s-7"
    assert expand("{people.clerk.login}", ctx(), Path("."), []) == ["anar", "bob"]
    assert expand("{people.nobody.login}", ctx(), Path("."), []) == []


def test_an_include_path_may_name_an_option(tmp_path):
    (tmp_path / "env").mkdir()
    (tmp_path / "env" / "full.yaml").write_text("MEM: 2g\n")
    assert expand("@env/{options.alpha.profile}.yaml", ctx(), tmp_path, []) == {"MEM": "2g"}
    with pytest.raises(TemplateError, match="no file env/lean.yaml"):
        expand("@env/{options.alpha.profile}.yaml", ctx(options={"alpha": {"profile": "lean"}}), tmp_path, [])


def _with_systems(fixtures, tmp_path, **changes):
    return load_template(copy_with(fixtures / "shop", tmp_path, changes))


def test_a_chosen_system_applies_its_ops_before_the_events(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path, **{
        "systems.2.parts": [],
        "systems.2.apply": [{"part": "alpha", "set": "gamma_on", "value": True},
                            {"part": "beta", "merge": "env", "value": {"A": "1", "SEED": "{seed}"}},
                            {"part": "beta", "append": "systems", "value": "gamma"}]})
    with_gamma = generate(t, {"seed": 5, "systems": ["alpha", "beta", "gamma"]}).world
    parts = {p["part"]: p["config"] for p in with_gamma["parts"]}
    assert "gamma" not in parts and parts["alpha"]["gamma_on"] is True
    assert parts["beta"]["env"] == {"A": "1", "SEED": 5} and parts["beta"]["systems"] == ["gamma"]
    without = {p["part"]: p["config"] for p in generate(t, {"systems": ["alpha", "beta"]}).world["parts"]}
    assert "gamma_on" not in without["alpha"] and "env" not in without["beta"]


def test_merge_keeps_what_the_config_had_and_needs_objects(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path, **{
        "systems.1.parts.0.config": {"env": {"KEEP": "k", "A": "0"}},
        "systems.1.apply": [{"part": "beta", "merge": "env", "value": {"A": "1"}}]})
    parts = {p["part"]: p["config"] for p in generate(t, {}).world["parts"]}
    assert parts["beta"]["env"] == {"KEEP": "k", "A": "1"}
    bad = load_template(copy_with(t.dir, tmp_path / "bad", {"systems.1.apply.0.value": "text"}))
    r = generate(bad, {})
    assert r.world is None and r.problems == [{"path": "systems[1].apply[0]", "message": "merge needs an object"}]


def test_a_check_that_needs_a_system_not_chosen_is_dropped(fixtures, tmp_path):
    checks = [{"name": "always", "check": {"part": "alpha", "key": "a"}},
              {"name": "with_beta", "needs": ["beta"], "check": {"part": "beta", "key": "b"}}]
    t = _with_systems(fixtures, tmp_path, **{"tasks.0.checks": checks})
    both = generate(t, {}).world["tasks"][0]["checks"]
    assert [c["name"] for c in both] == ["always", "with_beta"] and all("needs" not in c for c in both)
    r = generate(t, {"systems": ["alpha"]})
    assert [c["name"] for c in r.world["tasks"][0]["checks"]] == ["always"]
    assert {"what": "check with_beta of task alpha-task", "why": "needs the system beta, not chosen"} in r.dropped


def test_a_check_in_its_own_file_may_need_a_system(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path)
    (t.dir / "checks" / "alpha.yaml").write_text(yaml.safe_dump(
        [{"name": "with_beta", "needs": ["beta"], "check": {"part": "beta", "key": "b"}}]))
    assert generate(load_template(t.dir), {"systems": ["alpha"]}).world["tasks"][0]["checks"] == []
    (t.dir / "checks" / "alpha.yaml").write_text(yaml.safe_dump(
        [{"name": "x", "needs": ["omega"], "check": {"part": "beta", "key": "b"}}]))
    with pytest.raises(TemplateError, match="no system omega") as e:
        load_template(t.dir)
    assert e.value.path == "tasks[0].checks@checks/alpha.yaml[0].needs[0]"


def test_a_system_apply_names_known_parts(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path, **{"systems.1.apply": [{"part": "omega", "set": "x", "value": 1}]})
    r = generate(t, {})
    assert r.problems == [{"path": "systems[1].apply[0].part", "message": "no part omega in this world"}]


def test_an_event_may_be_written_in_its_own_file(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path)
    data = yaml.safe_load((t.dir / "template.yaml").read_text())
    (t.dir / "events").mkdir()
    (t.dir / "events" / "rush.yaml").write_text(yaml.safe_dump(data["events"][1]))
    data["events"][1] = "@events/rush.yaml"
    (t.dir / "template.yaml").write_text(yaml.safe_dump(data, sort_keys=False))
    loaded = load_template(t.dir)
    assert loaded.events[1]["id"] == "rush" and generate(loaded, {}).world == generate(t, {}).world
    (t.dir / "events" / "rush.yaml").write_text(yaml.safe_dump({**data["events"][0], "kind": "surprise"}))
    with pytest.raises(TemplateError) as e:
        load_template(t.dir)
    assert e.value.path.startswith("events[1]@events/rush.yaml")


def test_a_fixed_event_is_part_of_the_data_and_cannot_be_turned_off(fixtures, tmp_path):
    t = _with_systems(fixtures, tmp_path, **{"events.0.fixed": True, "events.0.default": True, "events.0.apply": []})
    assert [e["on"] for e in generate(t, {}).world["events"]][:1] == [True]
    r = generate(t, {"events": {"slow-supplier": {"on": False}}})
    assert r.problems == [{"path": "events.slow-supplier.on",
                           "message": "A supplier is slow is part of the template's data and can't be turned off"}]
    with pytest.raises(TemplateError, match="an event that isn't fixed applies something") as e:
        load_template(copy_with(t.dir, tmp_path / "bad", {"events.1.apply": []}))
    assert e.value.path == "events[1].apply"


def test_a_check_of_an_event_is_kept_only_while_the_event_is_on(fixtures, tmp_path):
    checks = [{"name": "late_noticed", "events": ["slow-supplier"], "check": {"part": "alpha", "key": "a"}}]
    t = _with_systems(fixtures, tmp_path, **{"tasks.0.checks": checks, "events.0.default": True})
    assert [c for c in generate(t, {}).world["tasks"][0]["checks"]] == [
        {"name": "late_noticed", "check": {"part": "alpha", "key": "a"}}]
    r = generate(t, {"events": {"slow-supplier": {"on": False}}})
    assert r.world["tasks"][0]["checks"] == []
    assert {"what": "check late_noticed of task alpha-task", "why": "needs the event slow-supplier, not on"} in r.dropped
    with pytest.raises(TemplateError, match="no event omega"):
        load_template(copy_with(t.dir, tmp_path / "bad", {"tasks.0.checks.0.events": ["omega"]}))


def test_a_patch_file_is_included_as_text(tmp_path):
    (tmp_path / "fix.diff").write_text("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b {company.name}\n")
    assert expand("@fix.diff", ctx(), tmp_path, []) == "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b Acme\n"
