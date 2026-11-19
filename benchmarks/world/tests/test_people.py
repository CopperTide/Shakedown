from worldbench.choices import Choices
from worldbench.people import apply_edits, generate_people


def _people(shop, raw):
    c, problems = Choices.parse(shop, raw)
    assert problems == []
    return generate_people(shop, c, shop.roles), c


def test_ids_follow_role_order_and_fields_follow_their_rules(shop):
    people, _ = _people(shop, {"size": 4})
    assert [(p["id"], p["role"]) for p in people] == [("p-001", "clerk"), ("p-002", "clerk"), ("p-003", "clerk"),
                                                    ("p-004", "lead")]
    assert [p["desk"] for p in people[:3]] == ["D1", "D2", "D1"]
    assert all(p["skills"] in (["till"], ["till", "stock"]) for p in people[:3])
    assert "desk" not in people[3] and len({p["name"] for p in people}) == 4


def test_the_same_seed_draws_the_same_people(shop):
    assert _people(shop, {"seed": 3})[0] == _people(shop, {"seed": 3})[0]


def test_edits_removals_and_additions_with_unique_logins(shop):
    people, c = _people(shop, {"size": 4, "people": {
        "edits": {"p-001": {"name": "Ana Ruiz"}, "p-002": {"name": "Ana Reyes"}},
        "added": [{"name": "Lee Park", "role": "lead"}], "removed": ["p-003"]}})
    out, problems = apply_edits(people, c)
    assert problems == []
    assert [(p["id"], p["name"], p["login"]) for p in out][:2] == [("p-001", "Ana Ruiz", "anar"),
                                                                 ("p-002", "Ana Reyes", "anar2")]
    assert [p["id"] for p in out] == ["p-001", "p-002", "p-004", "p-005"]
    assert out[-1] == {"id": "p-005", "name": "Lee Park", "login": "leep", "role": "lead", "added": True}


def test_an_edit_or_removal_of_no_one_is_a_problem(shop):
    people, c = _people(shop, {"size": 2, "people": {"edits": {"p-099": {"name": "X"}, "p-001": {"name": " "}},
                                                     "removed": ["p-077"]}})
    _, problems = apply_edits(people, c)
    assert problems == [{"path": "people.removed.p-077", "message": "no person p-077 in this world"},
                        {"path": "people.edits.p-099", "message": "no person p-099 in this world"},
                        {"path": "people.edits.p-001.name", "message": "a person needs a name"}]
