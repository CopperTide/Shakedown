"""`jira_act {person, action, args}`: what the world's people do in the tracker, called by links.
Each action follows the same rules as the REST API: `create_issue {project, summary, type?, description?,
assignee?, priority?, labels?}`, `transition {key, to}` (a status, or a transition's name),
`comment {key, body}`, `assign {key, to}` (null unassigns)."""
from arena_part.twin import Done

from . import records, rules, users
from .errors import JiraError
from .view import View


def _create(v: View, person: str, args: dict) -> Done:
    fields = {"project": {"key": args.get("project")}, "summary": args.get("summary"),
              "issuetype": {"name": args.get("type", "Task")}}
    for name in ("description", "labels"):
        if args.get(name) is not None:
            fields[name] = args[name]
    for name in ("assignee", "priority"):
        if args.get(name):
            fields[name] = {"name": args[name]}
    row = rules.create_issue(v, person, fields)
    return Done({"key": row["key"]}, writes=[f"issue/{row['key']}"])


def _transition(v: View, person: str, args: dict) -> Done:
    row = records.issue(v.db, args.get("key", ""))
    t = v.workflow.towards(row["status"], str(args.get("to", "")))
    rules.transition(v, person, row, t.id, {}, {})
    return Done({"key": row["key"], "status": t.to}, writes=[f"issue/{row['key']}"])


def _comment(v: View, person: str, args: dict) -> Done:
    row = records.issue(v.db, args.get("key", ""))
    c = rules.add_comment(v, person, row, str(args.get("body", "")))
    return Done({"key": row["key"], "id": str(c["id"])}, writes=[f"issue/{row['key']}"])


def _assign(v: View, person: str, args: dict) -> Done:
    row = records.issue(v.db, args.get("key", ""))
    rules.assign(v, row, args.get("to"))
    return Done({"key": row["key"], "assignee": args.get("to")}, writes=[f"issue/{row['key']}"])


ACTIONS = {"create_issue": _create, "transition": _transition, "comment": _comment, "assign": _assign}


def run(ctx, person: str, action: str, args: dict) -> Done:
    fn = ACTIONS.get(action)
    if fn is None:
        return Done(error=f"unknown action {action}")
    v = View(ctx)
    users.ensure(v.db, ctx.config, person or ctx.config["default_user"], v.now())
    try:
        return fn(v, person or ctx.config["default_user"], args)
    except JiraError as e:
        return Done(error=str(e))
