"""A simulated person's desk: decides the changes that are its own by fixed rules, records a review, and
gives the `decided` event a benchmark's link hands to the approvals system."""
from shakedown_part.jsonstate import Done
from shakedown_part.people import Desk


def _rules(config, state, action, payload):
    return (True, None) if payload.get("qty", 0) <= config["limit"] else (False, "Rejected: too many.")


DESK = Desk("Buyer", _rules, mine=lambda config, action: action in config["decides"],
            payload=lambda p: p if p is not None else {})


def review(args, config=None):
    state = {"reviews": {}}
    return DESK.review(config or {"limit": 5, "decides": ["purchase"]}, state, args), state


def test_a_change_of_its_own_is_decided_recorded_and_announced():
    done, state = review({"id": "c1", "action": "purchase", "payload": {"qty": 9}})
    assert done == Done({"decided": True, "approved": False, "note": "Rejected: too many."}, writes=["review/c1"],
                        events=[{"kind": "decided", "text": "Buyer rejected c1 (purchase)",
                                 "data": {"id": "c1", "approved": False, "note": "Rejected: too many."}}])
    assert state["reviews"]["c1"] == {"id": "c1", "action": "purchase", "payload": {"qty": 9}, "approved": False,
                                      "note": "Rejected: too many.", "by": "Buyer (simulated)"}
    done, state = review({"id": "c2", "action": "purchase", "payload": None})
    assert (done.result["approved"], state["reviews"]["c2"]["payload"]) == (True, {})
    assert done.events[0]["text"] == "Buyer approved c2 (purchase)"


def test_a_change_of_another_desk_is_left_alone():
    done, state = review({"id": "c3", "action": "release", "payload": {}})
    assert (done, state) == (Done({"decided": False}, reads=["review/c3"]), {"reviews": {}})
