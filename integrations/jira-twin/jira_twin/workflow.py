"""The workflow a config gives: statuses with their categories, and the transitions between them
(a transition without `from` is global: it is available from every status, as Jira's "any status" transitions are)."""
from dataclasses import dataclass

from .errors import JiraError

CATEGORIES = {"new": (2, "To Do", "blue-gray"), "indeterminate": (4, "In Progress", "yellow"),
              "done": (3, "Done", "green")}
DEFAULT = {"statuses": [{"name": "To Do", "category": "new"}, {"name": "In Progress", "category": "indeterminate"},
                        {"name": "Done", "category": "done"}],
           "transitions": [{"id": "11", "name": "To Do", "to": "To Do"},
                           {"id": "21", "name": "In Progress", "to": "In Progress"},
                           {"id": "31", "name": "Done", "to": "Done"}]}


@dataclass(frozen=True)
class Status:
    id: str
    name: str
    category: str


@dataclass(frozen=True)
class Transition:
    id: str
    name: str
    to: str
    sources: tuple | None


class Workflow:
    def __init__(self, spec: dict | None):
        spec = spec or DEFAULT
        self.statuses = [Status(str(10000 + i), s["name"], s.get("category", "new"))
                         for i, s in enumerate(spec["statuses"])]
        self.transitions = [Transition(str(t["id"]), t["name"], t["to"],
                                       tuple(t["from"]) if t.get("from") else None) for t in spec["transitions"]]

    def status(self, id_or_name: str) -> Status | None:
        want = str(id_or_name).lower()
        return next((s for s in self.statuses if s.id == want or s.name.lower() == want), None)

    def initial(self) -> Status:
        return self.statuses[0]

    def available(self, current: str) -> list[Transition]:
        """The transitions from status `current`."""
        return [t for t in self.transitions if t.sources is None or current in t.sources]

    def transition(self, current: str, transition_id: str) -> Transition:
        found = next((t for t in self.available(current) if t.id == str(transition_id)), None)
        if found is None:
            raise JiraError(400, [f"Transition id '{transition_id}' is not valid for this issue."])
        return found

    def towards(self, current: str, target: str) -> Transition:
        """The available transition into status `target` (or named `target`)."""
        avail = self.available(current)
        want = target.lower()
        found = next((t for t in avail if t.to.lower() == want), None) or \
            next((t for t in avail if t.name.lower() == want), None)
        if found is None:
            raise JiraError(400, [f"No transition from '{current}' to '{target}'."])
        return found


def category(key: str) -> tuple[int, str, str]:
    """`(id, name, colour)` of a status category key."""
    return CATEGORIES[key]


def category_by(id_or_key: str) -> str | None:
    """The category key named by its id, key or name."""
    want = str(id_or_key).lower()
    return next((k for k, (i, name, _) in CATEGORIES.items() if want in (str(i), k, name.lower())), None)
