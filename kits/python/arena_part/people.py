"""Simulated people in a world: a desk that decides proposed changes by fixed rules over its part's records."""
from dataclasses import dataclass
from typing import Callable

from .jsonstate import Done, event


def _as_given(payload):
    return payload


@dataclass(frozen=True)
class Desk:
    """A simulated person who decides the changes that are theirs (`mine(config, action)`) with `decide(config,
    state, action, payload) -> (approved, note)`, records each in `state["reviews"]` and gives a `decided`
    event that a benchmark's link hands to the approvals system. `title` names them in the event and the
    review; `payload` reads a change's payload before the rules see it."""
    title: str
    decide: Callable
    mine: Callable
    payload: Callable = _as_given

    def review(self, config, state, args) -> Done:
        """Decides the change `args` describes when it is this desk's; leaves it alone otherwise."""
        change_id, action = args["id"], args["action"]
        key = f"review/{change_id}"
        if not self.mine(config, action):
            return Done({"decided": False}, reads=[key])
        payload = self.payload(args["payload"])
        approved, note = self.decide(config, state, action, payload)
        state["reviews"][change_id] = {"id": change_id, "action": action, "payload": payload,
                                       "approved": approved, "note": note, "by": f"{self.title} (simulated)"}
        verdict = "approved" if approved else "rejected"
        decided = event("decided", f"{self.title} {verdict} {change_id} ({action})",
                        {"id": change_id, "approved": approved, "note": note})
        return Done({"decided": True, "approved": approved, "note": note}, writes=[key], events=[decided])
