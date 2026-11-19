"""The rule format: data checked by `rules.schema.json` and a few cross-checks, loaded into rules the engine runs."""
import copy
import re
from pathlib import Path

import pytest
import yaml

from people_part import rules
from support import FIXTURES, rules_doc

REPO = Path(__file__).resolve().parents[3]
TEMPLATE_RULES = [*REPO.glob("benchmarks/world/library/*/rules/*.yaml"),
                  *REPO.glob("benchmarks/world/templates/*/rules/*.yaml")]


def refused(doc) -> str:
    out = rules.load(doc)
    assert isinstance(out, rules.RulesError), out
    return str(out)


def with_rule(**changes) -> dict:
    doc = rules_doc("reviewer")
    doc["rules"][0].update(changes)
    return doc


@pytest.mark.parametrize("path", sorted(FIXTURES.glob("*/*.yaml")) + sorted(TEMPLATE_RULES),
                         ids=lambda p: str(p.relative_to(REPO)))
def test_every_fixture_and_template_rules_file_validates(path):
    doc = yaml.safe_load(path.read_text())
    out = rules.load(doc)
    assert isinstance(out, rules.RoleRules), str(out)
    assert out.role == doc["role"] and [r.id for r in out.rules] == [r["id"] for r in doc["rules"]]


def test_the_fixtures_cover_the_four_roles():
    assert {p.stem for p in FIXTURES.glob("rules/*.yaml")} == {"developer", "reviewer", "oncall", "pm"}


def test_a_doc_of_another_format_is_refused():
    doc = rules_doc("pm")
    doc["format"] = "shakedown-people/0"
    assert refused(doc).startswith("format:")
    assert refused([]).startswith("(the rules):")


def test_unknown_keys_and_two_triggers_are_refused():
    assert "rules[0]" in refused(with_rule(cadence="often"))
    assert "rules[0]" in refused(with_rule(every={"minutes": 5}))
    no_trigger = with_rule()
    del no_trigger["rules"][0]["notice"]
    assert "rules[0]" in refused(no_trigger)


def test_bad_times_and_ranges_are_refused():
    assert refused(with_rule(after={"minutes": [60, 15]})) == "rules[0].after.minutes: 60 is after 15"
    doc = rules_doc("pm")
    doc["rules"][0]["every"] = {"at": "24:00"}
    assert refused(doc).startswith("rules[0].every")
    doc["rules"][0]["every"] = {"minutes": 0}
    assert refused(doc).startswith("rules[0].every")


def test_a_pick_of_lines_the_doc_lacks_is_refused():
    doc = rules_doc("reviewer")
    del doc["lines"]["red"]
    assert refused(doc) == "rules[0].do[1].act.args.body: no lines named red"


def test_rule_ids_are_unique_and_else_comes_last():
    doc = rules_doc("pm")
    doc["rules"][1]["id"] = doc["rules"][0]["id"]
    assert refused(doc) == "rules[1].id: ask-status is used twice"
    doc = rules_doc("reviewer")
    doc["rules"][0]["do"].reverse()
    assert refused(doc) == "rules[0].do[0]: else must be the last branch"


def test_refresh_needs_a_notice_rule():
    doc = rules_doc("pm")
    doc["rules"][0]["refresh"] = copy.deepcopy(rules_doc("reviewer")["rules"][0]["refresh"])
    assert refused(doc) == "rules[0].refresh: only a rule that answers a notice can be refreshed"


def test_loading_gives_the_rules_in_order_with_world_times():
    pm = rules.load(rules_doc("pm"))
    status, ticket, done = pm.rules
    assert status.every == rules.Every(at=(570, 960)) and status.who == "one" and status.limit_per_day == 2
    assert ticket.after == (0, 30 * 60_000) and done.notice.where == {"/to": "In Review"}
    assert [a.action for b in done.branches for a in b.acts] == ["transition"]


def test_the_behaviours_are_data_not_code():
    """What people do (systems, notices, actions, channels, times) lives in rules files, never in the engine."""
    code = "\n".join(p.read_text() for p in (REPO / "integrations/people/people_part").glob("*.py"))
    words = {"review_requested", "incident_triggered", "alerts_cleared", "review_submitted", "issue_transitioned",
             "github", "jira", "slack", "pagerduty", "#incidents", "APPROVE", "09:30", "16:00", "In Review"}
    assert sorted(w for w in words if re.search(rf"(?<![\w#]){re.escape(w)}(?!\w)", code)) == []
