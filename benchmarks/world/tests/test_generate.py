import pytest
from support import copy_with

from worldbench.generate import generate, world_json
from worldbench.template import load_template


def test_the_same_template_choices_and_seed_give_the_same_bytes(shop):
    a = world_json(generate(shop, {"seed": 7, "company": {"name": "Acme Tools"}}).world)
    b = world_json(generate(shop, {"seed": 7, "company": {"name": "Acme Tools"}}).world)
    assert a == b and "${WORLD_DIR}" in a and str(shop.dir) not in a


def test_another_seed_gives_other_names_but_the_same_counts(shop):
    p7 = generate(shop, {"seed": 7}).world["people"]
    p8 = generate(shop, {"seed": 8}).world["people"]
    assert [p["id"] for p in p7] == [p["id"] for p in p8] and [p["name"] for p in p7] != [p["name"] for p in p8]


def test_dropping_a_system_drops_what_needs_it(shop):   # Review Focus 3
    r = generate(shop, {"systems": ["alpha"]})
    assert {"what": "task beta-task", "why": "needs the system beta, not chosen"} in r.dropped
    assert all(p["part"] != "beta" for p in r.world["parts"])
    assert {"what": "role lead", "why": "needs the system beta, not chosen"} in r.dropped
    assert {"what": "link alpha shipped → beta", "why": "needs the system beta, not chosen"} in r.dropped
    assert [e["id"] for e in r.world["events"]] == ["slow-supplier"]


def test_a_world_with_no_task_is_a_problem(shop_only_beta_tasks):   # Review Focus 3
    r = generate(shop_only_beta_tasks, {"systems": ["alpha"]})
    assert r.world is None and r.problems == [{"path": "tasks", "message": "every task needs a system that is not chosen"}]


def test_counts_follow_size_and_edits_rename_by_id(shop):
    w = generate(shop, {"size": 20, "people": {"edits": {"clerk-002": {"name": "Ana Ruiz"}}}}).world
    assert len([p for p in w["people"] if p["role"] == "clerk"]) == 15     # share 0.75 of 20
    assert next(p for p in w["people"] if p["id"] == "clerk-002")["name"] == "Ana Ruiz"


def test_an_edit_of_a_missing_person_is_a_problem(shop):   # Review Focus 5
    r = generate(shop, {"size": 2, "people": {"edits": {"clerk-099": {"name": "X"}}}})
    assert {"path": "people.edits.clerk-099", "message": "no person clerk-099 in this world"} in r.problems
    assert r.world is None


def test_an_event_applies_into_its_parts_config_with_its_params(shop):
    w = generate(shop, {"events": {"slow-supplier": {"on": True, "params": {"days": 4}}}}).world
    alpha = next(p for p in w["parts"] if p["part"] == "alpha")
    assert alpha["config"]["delays"] == [{"id": "slow-supplier", "days": 4}]


def test_the_default_world(shop):
    r = generate(shop, {})
    w = r.world
    assert r.problems == [] and r.dropped == [] and r.files == [(shop.dir / "seed", "files/seed/")]
    assert (w["name"], w["title"], w["seed"]) == ("example-co", "Example Co", 1)
    assert w["company"] == {"name": "Example Co", "slug": "example-co", "key": "EC", "domain": "example-co.test",
                            "clock_start": "2026-01-05T09:00:00"}
    assert w["template"] == {"id": "shop", "name": "Shop", "version": "1.0.0"}
    assert [(p["part"], p["integration"]) for p in w["parts"]] == [("alpha", "toy-a"), ("beta", "toy-b")]
    alpha, beta = w["parts"][0]["config"], w["parts"][1]["config"]
    assert alpha == {"company": "Example Co", "profile": "lean", "seed": "${WORLD_DIR}/files/seed/", "size": 10}
    assert [p["role"] for p in beta["leads"]] == ["lead"] and len(beta["staff"]) == 8


def test_a_default_event_sets_adds_setup_and_links(shop):
    w = generate(shop, {"events": {"rush": {"params": {"desk": "D2"}}}}).world
    assert w["parts"][1]["config"]["rush"] == {"desk": "D2"}
    assert all({"part": "beta", "tool": "rush", "args": {}} in t["setup"] for t in w["tasks"])
    assert w["links"] == [{"from": "alpha", "event": "shipped", "to": "beta", "tool": "log", "args": {}},
                          {"from": "beta", "event": "rushed", "to": "alpha", "tool": "note", "args": {"text": "/text"}}]
    assert w["events"] == [
        {"id": "slow-supplier", "label": "A supplier is slow", "kind": "trap", "on": False, "params": {"days": 2}},
        {"id": "rush", "label": "A rush order", "kind": "event", "on": True, "params": {"desk": "D2"}}]


def test_tasks_carry_their_checks_and_the_company_clock(shop):
    task = generate(shop, {"company": {"name": "Acme"}}).world["tasks"][0]
    assert task["reason"] == "Ship order 1 for Acme" and task["clock_start"] == "2026-01-05T09:00:00"
    assert task["ticket"] == "Order 1 of Acme must ship today ({urgent}).\n"
    assert task["checks"][0]["name"] == "shipped" and task["setup"][0] == {"part": "alpha", "tool": "reset", "args": {}}


def test_gates_pass_through_as_written_without_needs(shop):
    assert generate(shop, {}).world["gates"] == [
        {"tool": "ship", "part": "alpha", "key": "orders/{order_id}", "require": {"/approved": True}}]


def test_a_system_brings_what_it_requires(shop):
    w = generate(shop, {"systems": ["gamma"]}).world
    assert [p["part"] for p in w["parts"]] == ["alpha", "beta", "gamma"]
    assert w["choices"]["systems"] == ["alpha", "beta", "gamma"]


@pytest.mark.parametrize("raw, path, message", [
    ({"systems": ["delta"]}, "systems[0]", "no system delta"),
    ({"size": 41}, "size", "Size is between 2 and 40"),
    ({"company": {"name": "  "}}, "company.name", "Give the company a name"),
    ({"options": {"alpha": {"profile": "huge"}}}, "options.alpha.profile", "huge is not one of lean, full"),
    ({"options": {"alpha": {"mode": "x"}}}, "options.alpha.mode", "alpha has no option mode"),
    ({"events": {"slow-supplier": {"on": True, "params": {"days": 10}}}}, "events.slow-supplier.params.days",
     "days is between 1 and 9"),
    ({"events": {"rush": {"params": {"desk": "D9"}}}}, "events.rush.params.desk", "D9 is not one of D1, D2"),
    ({"events": {"storm": {"on": True}}}, "events.storm", "no event storm"),
    ({"people": {"counts": {"clerk": 0}}}, "people.counts.clerk", "Clerks need at least 1"),
    ({"people": {"counts": {"boss": 2}}}, "people.counts.boss", "no role boss"),
    ({"systems": ["alpha"], "people": {"added": [{"name": "Lee Park", "role": "lead"}]}}, "people.added[0].role",
     "no role lead in this world"),
    ({"seed": "x"}, "seed", "seed must be a whole number"),
])
def test_choices_that_dont_fit_are_problems_with_paths(shop, raw, path, message):
    r = generate(shop, raw)
    assert r.world is None and {"path": path, "message": message} in r.problems


def test_hand_set_counts_and_too_few_names(shop, tmp_path):
    w = generate(shop, {"people": {"counts": {"clerk": 3, "lead": 2}}}).world
    assert [p["role"] for p in w["people"]] == ["clerk"] * 3 + ["lead"] * 2
    small = load_template(copy_with(shop.dir, tmp_path, {"names": {"first": ["Ada"], "last": ["Ames", "Bell"]}}))
    r = generate(small, {"size": 4})
    assert r.problems == [{"path": "names", "message": "the name pools hold 2 full names; this world needs 4"}]


def test_a_template_error_while_filling_is_a_problem(fixtures):
    r = generate(load_template(fixtures / "bad-placeholder"), {})
    assert r.world is None and r.problems == [{"path": "tasks[0].ticket", "message": "unknown placeholder {company.ceo}"}]


def _of(world, role):
    return [(p["id"], p["name"]) for p in world["people"] if p["role"] == role]


def test_an_edit_survives_a_count_change_in_another_role(shop):   # Review Focus 5, per-role ids
    edits = {"lead-001": {"name": "Ana Ruiz"}}
    before = generate(shop, {"size": 10, "people": {"edits": edits}}).world
    after = generate(shop, {"size": 10, "people": {"edits": edits, "counts": {"clerk": 12}}}).world
    assert _of(after, "lead") == _of(before, "lead") == [("lead-001", "Ana Ruiz")]
    assert _of(after, "clerk")[:8] == _of(before, "clerk") and len(_of(after, "clerk")) == 12
    assert [i for i, _ in _of(after, "clerk")] == [f"clerk-{n:03d}" for n in range(1, 13)]


def test_lowering_a_count_drops_only_that_roles_highest_ids(shop):
    full = generate(shop, {"people": {"counts": {"clerk": 6, "lead": 3}}}).world
    fewer = generate(shop, {"people": {"counts": {"clerk": 4, "lead": 3}}}).world
    assert _of(fewer, "clerk") == _of(full, "clerk")[:4] and _of(fewer, "lead") == _of(full, "lead")
    assert world_json(fewer) == world_json(generate(shop, {"people": {"counts": {"clerk": 4, "lead": 3}}}).world)


def test_a_role_with_too_few_names_of_its_own_is_a_problem(shop, tmp_path):
    small = load_template(copy_with(shop.dir, tmp_path, {"names": {"first": ["Ada", "Ben"], "last": ["Ames", "Bell", "Cole"]}}))
    r = generate(small, {"people": {"counts": {"clerk": 5, "lead": 1}}})
    assert r.world is None and r.problems[0]["path"] == "names"
    assert r.problems[0]["message"].startswith("the name pools give the role clerk ")
