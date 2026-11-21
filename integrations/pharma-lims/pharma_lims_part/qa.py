"""The simulated QA head: decides the changes of the actions the config gives it (`decides`), by fixed rules
over the lab's records, and gives a `decided` event that a benchmark's link hands to the approvals system.
Changes of other actions are not this desk's: it leaves them alone."""
from datetime import date

from shakedown_part.people import Desk
from shakedown_part.params import Refusal

from . import lab


def _release(config, state, payload):
    """(approved, note) for releasing `payload.batch`."""
    try:
        results = lab.batch_results(config, state, payload.get("batch", ""))
    except Refusal as e:
        return False, f"Rejected: {e}."
    batch = results["batch"]
    if date.fromisoformat(results["expiry"]) < date.fromisoformat(config["today"]):
        return False, f"Rejected: {batch} expired on {results['expiry']}."
    if results["em_excursions"]:
        ids = ", ".join(results["em_excursions"])
        return False, f"Rejected: {ids} exceeded the Grade A action limit; {batch} stays on hold until the investigation closes."
    if results.get("failed"):
        return False, f"Rejected: {batch} failed {', '.join(results['failed'])}; it can't be released."
    if not results["all_passed"]:
        open_tests = [t["test"] for t in results["tests"] if t["status"] != "pass"]
        return False, f"Rejected: {batch}'s release testing is not complete ({', '.join(open_tests)})."
    return True, None


def _decision(config, state, action, payload):
    if not isinstance(payload, dict) or not payload.get("batch"):
        return False, "Rejected: the payload must name the batch."
    if action == "batch_release":
        return _release(config, state, payload)
    return True, None  # a hold or a deviation only makes things safer


def _mine(config, action) -> bool:
    return action in config.get("decides", [])


DESK = Desk("QA head", _decision, _mine)
review = DESK.review
