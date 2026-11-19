"""What a Jira instance lists and an issue refers to: issue types, priorities (highest first),
resolutions, link types and fields. A config may give its own types and priorities."""
from dataclasses import dataclass

TYPES = [{"name": "Epic", "level": 1}, {"name": "Story"}, {"name": "Task"}, {"name": "Bug"},
         {"name": "Sub-task", "subtask": True, "level": -1}]
PRIORITIES = ["Highest", "High", "Medium", "Low", "Lowest"]
DEFAULT_PRIORITY = "Medium"
RESOLUTIONS = ["Done", "Won't Do", "Duplicate"]
LINK_TYPES = [("10000", "Blocks", "is blocked by", "blocks"), ("10001", "Cloners", "is cloned by", "clones"),
              ("10002", "Duplicate", "is duplicated by", "duplicates"), ("10003", "Relates", "relates to", "relates to")]
SPRINT_FIELD = "customfield_10020"
RANK_FIELD = "customfield_10019"


@dataclass(frozen=True)
class Named:
    id: str
    name: str
    subtask: bool = False
    level: int = 0


class Meta:
    def __init__(self, config: dict):
        self.types = [Named(str(10001 + i), t["name"], bool(t.get("subtask")), t.get("level", 0))
                      for i, t in enumerate(config.get("issue_types") or TYPES)]
        self.priorities = [Named(str(i + 1), p) for i, p in enumerate(config.get("priorities") or PRIORITIES)]
        self.resolutions = [Named(str(10000 + i), r) for i, r in enumerate(RESOLUTIONS)]
        self.link_types = list(LINK_TYPES)

    @staticmethod
    def _find(items, id_or_name):
        want = str(id_or_name).lower()
        return next((x for x in items if x.id == want or x.name.lower() == want), None)

    def type(self, id_or_name) -> Named | None:
        return self._find(self.types, id_or_name)

    def priority(self, id_or_name) -> Named | None:
        return self._find(self.priorities, id_or_name)

    def resolution(self, id_or_name) -> Named | None:
        return self._find(self.resolutions, id_or_name)

    def priority_rank(self, name: str) -> int:
        """0 for the highest priority; larger is lower."""
        p = self.priority(name)
        return self.priorities.index(p) if p else len(self.priorities)

    def link_type(self, id_or_name) -> tuple | None:
        want = str(id_or_name).lower()
        return next((t for t in self.link_types if want in (t[0], t[1].lower())), None)


def _field(id_, name, schema, clauses, custom=False, orderable=True):
    return {"id": id_, "key": id_, "name": name, "custom": custom, "orderable": orderable, "navigable": True,
            "searchable": True, "clauseNames": clauses, "schema": schema}


def fields() -> list[dict]:
    """`GET /field`: the system fields the twin keeps, and the agile Sprint and Rank fields."""
    sys = lambda t, s, **kw: {"type": t, "system": s, **kw}  # noqa: E731
    return [_field("summary", "Summary", sys("string", "summary"), ["summary"]),
            _field("description", "Description", sys("string", "description"), ["description"]),
            _field("issuetype", "Issue Type", sys("issuetype", "issuetype"), ["issuetype", "type"]),
            _field("project", "Project", sys("project", "project"), ["project"]),
            _field("status", "Status", sys("status", "status"), ["status"], orderable=False),
            _field("priority", "Priority", sys("priority", "priority"), ["priority"]),
            _field("assignee", "Assignee", sys("user", "assignee"), ["assignee"]),
            _field("reporter", "Reporter", sys("user", "reporter"), ["reporter"]),
            _field("creator", "Creator", sys("user", "creator"), ["creator"], orderable=False),
            _field("labels", "Labels", sys("array", "labels", items="string"), ["labels"]),
            _field("resolution", "Resolution", sys("resolution", "resolution"), ["resolution"]),
            _field("resolutiondate", "Resolved", sys("datetime", "resolutiondate"), ["resolutiondate", "resolved"],
                   orderable=False),
            _field("created", "Created", sys("datetime", "created"), ["created", "createdDate"], orderable=False),
            _field("updated", "Updated", sys("datetime", "updated"), ["updated", "updatedDate"], orderable=False),
            _field("duedate", "Due date", sys("date", "duedate"), ["due", "duedate"]),
            _field("parent", "Parent", sys("issuelink", "parent"), ["parent"]),
            _field("issuelinks", "Linked Issues", sys("array", "issuelinks", items="issuelinks"), ["issueLink"]),
            _field("comment", "Comment", sys("comments-page", "comment"), ["comment"]),
            _field(SPRINT_FIELD, "Sprint", {"type": "array", "items": "json",
                                            "custom": "com.pyxis.greenhopper.jira:gh-sprint", "customId": 10020},
                   ["cf[10020]", "Sprint", "sprint"], custom=True),
            _field(RANK_FIELD, "Rank", {"type": "any", "custom": "com.pyxis.greenhopper.jira:gh-lexo-rank",
                                        "customId": 10019}, ["cf[10019]", "Rank", "rank"], custom=True)]


def settable() -> set:
    """The fields create and edit accept."""
    return {"project", "summary", "description", "issuetype", "priority", "assignee", "reporter", "labels",
            "duedate", "parent", SPRINT_FIELD}
