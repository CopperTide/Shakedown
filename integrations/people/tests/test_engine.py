"""The engine: people answer notices after seeded delays and act on schedules, in world time, the same way every
time for the same seed, rules and notices; a save keeps what each person still means to do."""
import time

import pytest

from shakedown_part import PartError
from people_part import schedule
from support import DAY, EPOCH, HOUR, MIN, Engine, acts, config

ASKED = {"reviewer": "rev1", "repo": "acme/app", "number": 3, "author": "dev1", "title": "T", "checks": "success"}


def reviewers(**kw) -> Engine:
    return Engine(roles=("reviewer",), **kw)


def asked(e: Engine, at_ms=0, **data):
    return e.notice("github", "review_requested", {**ASKED, **data}, at_ms)


def run_hours(e: Engine, hours: int) -> list:
    asked(e)
    return acts(e.run(hours * HOUR))


def test_the_same_seed_gives_the_same_acts():
    a, b = Engine(seed=7), Engine(seed=7)
    assert run_hours(a, 30) == run_hours(b, 30)
    assert run_hours(Engine(seed=8), 30) != run_hours(Engine(seed=7), 30)


def test_a_review_request_reaches_only_the_named_reviewer():
    e = reviewers()
    assert asked(e)["writes"] == ["person/reviewer-001"]
    [(_, who, system, action, args)] = [a for a in acts(e.run(2 * HOUR)) if a[2] == "github"]
    assert (who, system, action) == ("rev1", "github", "review")
    assert args == {"repo": "acme/app", "number": 3, "event": "APPROVE", "body": args["body"]}
    assert args["body"] in {"LGTM", "Looks good, ship it", "Approved, thanks"}
    assert e.notice("github", "review_requested", {**ASKED, "reviewer": "nobody"})["writes"] == []


def test_after_delays_fall_in_range_in_world_time():
    seen = set()
    for seed in range(40):
        e = reviewers(seed=seed)
        asked(e, at_ms=10 * MIN)
        [(at, *_)] = [a for a in acts(e.run(2 * HOUR)) if a[2] == "github"]
        assert EPOCH + 25 * MIN <= at <= EPOCH + 70 * MIN
        seen.add(at)
    assert len(seen) > 20


def test_the_delay_is_drawn_from_the_seed_person_rule_and_count():
    e = Engine(seed=7)
    rule = e.part.world_of(e.instance).rules["reviewer"].rules[0]
    person = {"id": "reviewer-001"}
    assert schedule.due(7, person, rule, 0, 0) == schedule.due(7, person, rule, 0, 0)
    assert len({schedule.due(7, person, rule, 0, n) for n in range(10)}) > 5
    assert all(15 * MIN <= schedule.due(s, person, rule, 0, 0) <= 60 * MIN for s in range(50))


def test_every_at_fires_once_per_world_day():
    e = Engine(roles=("pm",))
    status = [a for a in acts(e.run(3 * DAY, step_ms=7 * MIN)) if a[2] == "slack"]
    assert [a[0] - EPOCH for a in status] == [d * DAY + t for d in range(3) for t in (9 * HOUR + 30 * MIN, 16 * HOUR)]
    assert {a[1] for a in status} <= {"pm1", "pm2"} and len({a[1] for a in status}) == 2
    assert all(a[3:] == ("post", {"channel": "#eng", "text": a[4]["text"]}) for a in status)


def test_a_daily_limit_holds_the_rule_back():
    cfg = config(roles=("pm",))
    cfg["rules"]["pm"]["rules"][0]["limit"] = {"per_day": 1}
    status = [a for a in acts(Engine(cfg).run(2 * DAY, step_ms=HOUR)) if a[2] == "slack"]
    assert [a[0] - EPOCH for a in status] == [9 * HOUR + 30 * MIN, DAY + 9 * HOUR + 30 * MIN]


def test_how_the_advances_are_split_changes_nothing():
    whole, steps = Engine(), Engine()
    asked(whole), asked(steps)
    assert acts(whole.advance(DAY)) == acts(steps.run(DAY, step_ms=13 * MIN))
    assert whole.docs() == steps.docs()


def test_a_paused_clock_pauses_people(monkeypatch):
    e = reviewers()
    asked(e)
    assert e.advance(10 * MIN) == []
    later = time.monotonic() + 10 * DAY / 1000
    monkeypatch.setattr(time, "monotonic", lambda: later)
    monkeypatch.setattr(time, "time", lambda: later)
    assert [e.advance(10 * MIN) for _ in range(5)] == [[]] * 5
    assert e.docs("person/reviewer-001")["person/reviewer-001"]["pending"]
    assert [a[3] for a in acts(e.advance(2 * HOUR))] == ["review"]


def test_speed_scales_people_in_wall_time():
    """A driver whose clock runs at `speed` world ms per wall ms sees the same acts at the same world times, and
    so each one `speed` times sooner in wall time."""
    def run(speed):
        e, wall, out = reviewers(speed=speed), 0, []
        asked(e)
        while not out:
            wall += 1000
            out = [(wall, a) for a in acts(e.advance(int(wall * speed)))]
        return out[0]

    (wall_1, act_1), (wall_30, act_30) = run(1.0), run(30.0)
    assert act_1 == act_30 and wall_30 * 30 - wall_1 < 30 * 1000


def test_reviewers_ask_for_changes_when_checks_are_red_and_wait_while_they_run():
    def first(**data):
        e = reviewers()
        asked(e, **data)
        return [a for a in acts(e.run(2 * HOUR)) if a[2] == "github"][0][3:]

    assert first(checks="failure")[1]["event"] == "REQUEST_CHANGES"
    waiting = "Checks are still running, I'll look again when they finish."
    assert first(checks="pending") == ("comment", {"repo": "acme/app", "number": 3, "body": waiting})


def test_checks_that_finish_before_the_review_update_what_the_reviewer_sees():
    e = reviewers()
    asked(e, checks="pending")
    e.notice("github", "checks_completed", {"repo": "acme/app", "sha": "a" * 40, "conclusion": "failure",
                                            "prs": [2, 3]}, at_ms=5 * MIN)
    e.notice("github", "checks_completed", {"repo": "acme/other", "sha": "b" * 40, "conclusion": "success",
                                            "prs": [3]}, at_ms=6 * MIN)
    [(_, _, _, action, args)] = [a for a in acts(e.run(2 * HOUR)) if a[2] == "github"]
    assert (action, args["event"]) == ("review", "REQUEST_CHANGES")


def test_on_call_acknowledges_within_five_minutes_posts_and_resolves_when_alerts_clear():
    e = Engine(roles=("oncall",))
    page = {"incident": "P1", "service": "orders", "assignee": "oc1", "summary": "Errors above 1%"}
    e.notice("pagerduty", "incident_triggered", page, at_ms=HOUR)
    first = acts(e.run(HOUR + 6 * MIN))
    assert [a[1:4] for a in first] == [("oc1", "pagerduty", "ack"), ("oc1", "slack", "post")]
    assert first[0][0] == first[1][0] <= EPOCH + HOUR + 5 * MIN
    assert first[1][4]["channel"] == "#incidents" and first[1][4]["text"].startswith("P1 on orders: Errors above 1%.")
    e.notice("pagerduty", "alerts_cleared", page, at_ms=2 * HOUR)
    assert [a[2:5] for a in acts(e.run(3 * HOUR))][0] == ("pagerduty", "resolve", {"incident": "P1"})


def test_product_managers_write_tickets_ask_for_status_and_move_done_work():
    e = Engine(roles=("pm",))
    e.notice("jira", "issue_transitioned", {"key": "ACME-4", "from": "In Progress", "to": "In Review", "by": "x"},
             at_ms=8 * HOUR)
    e.notice("jira", "issue_transitioned", {"key": "ACME-5", "from": "To Do", "to": "In Progress", "by": "x"},
             at_ms=8 * HOUR)
    day = acts(e.run(DAY, step_ms=5 * MIN))
    jira = [a for a in day if a[2] == "jira"]
    assert [(a[3], a[4]) for a in jira] == [("transition", {"key": "ACME-4", "to": "Done"}),
                                            ("create_issue", {"project": "ACME", "summary": jira[1][4]["summary"],
                                                              "type": "Task"})]
    assert 8 * HOUR + 20 * MIN <= jira[0][0] - EPOCH <= 8 * HOUR + 90 * MIN
    assert 11 * HOUR <= jira[1][0] - EPOCH <= 11 * HOUR + 30 * MIN
    assert len([a for a in day if a[2] == "slack"]) == 2


def test_developers_open_their_own_pull_request_and_answer_review_comments_within_the_hour():
    e = Engine(roles=("developer",))
    [opened] = acts(e.run(31 * MIN))
    assert opened[0] == EPOCH + 30 * MIN and opened[3] == "open_pr"
    assert opened[4]["branch"] == f"{opened[1]}-notes" and opened[4]["patch"].startswith("diff --git a/notes.txt")
    review = {"repo": "acme/app", "number": 1, "author": "dev2", "reviewer": "rev1", "body": "Rename it"}
    e.notice("github", "review_submitted", {**review, "state": "APPROVED"}, at_ms=HOUR)
    e.notice("github", "review_submitted", {**review, "state": "CHANGES_REQUESTED"}, at_ms=HOUR)
    [answer] = acts(e.run(3 * HOUR))
    assert answer[1:4] == ("dev2", "github", "comment") and answer[0] - EPOCH <= 2 * HOUR
    assert answer[4]["body"].startswith("@rev1 ")


def test_acts_on_systems_the_world_lacks_are_skipped():
    e = Engine(roles=("oncall",), systems=["pagerduty"])
    e.notice("pagerduty", "incident_triggered", {"incident": "P1", "service": "s", "assignee": "oc1", "summary": "x"})
    assert [a[2:4] for a in acts(e.run(HOUR))] == [("pagerduty", "ack")]
    assert e.docs("person/oncall-001")["person/oncall-001"]["skipped"] == 1


def test_docs_show_each_persons_pending_and_acted_and_the_log():
    e = Engine(roles=("reviewer", "pm"))
    asked(e, at_ms=0)
    doc = e.docs("person/reviewer-001")["person/reviewer-001"]
    assert (doc["name"], doc["role"], doc["login"], doc["acted"]) == ("Rev One", "reviewer", "rev1", 0)
    [pending] = doc["pending"]
    assert pending["rule"] == "review-when-asked" and EPOCH + 15 * MIN <= pending["at_ms"] <= EPOCH + 60 * MIN
    e.run(HOUR)
    docs = e.docs("person/reviewer-001", "log/acts")
    assert docs["person/reviewer-001"]["pending"] == [] and docs["person/reviewer-001"]["acted"] == 1
    assert docs["log/acts"]["entries"][0] == {"at_ms": pending["at_ms"], "person": "rev1", "id": "reviewer-001",
                                              "rule": "review-when-asked", "system": "github", "action": "review"}
    assert e.docs("role/pm")["role/pm"]["next"][0] == {"rule": "ask-status", "at_ms": EPOCH + 9 * HOUR + 30 * MIN}


def test_pending_acts_survive_save_and_restore(saves_root):
    straight = Engine()
    asked(straight)
    expected = acts(straight.run(2 * DAY, step_ms=10 * MIN))
    e = Engine()
    asked(e)
    before = acts(e.run(20 * MIN, step_ms=10 * MIN))
    e.save("s1", saves_root)
    pending = e.docs("person/reviewer-001")["person/reviewer-001"]["pending"]
    fork = Engine(instance="f", save="s1", clock_ms=20 * MIN, seed=7)
    assert fork.docs("person/reviewer-001")["person/reviewer-001"]["pending"] == pending
    assert before + acts(fork.run(2 * DAY, step_ms=10 * MIN)) == expected
    assert e.part.inspect({"save": "s1"}, ["person/reviewer-001"])["docs"]["person/reviewer-001"]["pending"] == pending


def test_a_bad_config_or_notice_is_refused():
    with pytest.raises(PartError, match="rules.pm: rules\\[0\\]"):
        cfg = config(roles=("pm",))
        cfg["rules"]["pm"]["rules"][0]["every"] = {"at": "9:30"}
        Engine(cfg)
    with pytest.raises(PartError, match="rules.pm: the doc is for the role reviewer"):
        cfg = config(roles=("pm",))
        cfg["rules"]["pm"]["role"] = "reviewer"
        Engine(cfg)
    assert Engine().notice("github", 3, {})["error"] == "kind must be of type string"
