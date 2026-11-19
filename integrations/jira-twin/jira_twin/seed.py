"""A new instance from its config: users, projects, boards, sprints and issues (times relative to
world now, such as `-3d`). Seeding reports no notices."""
from . import records, rules, times, users
from .meta import SPRINT_FIELD
from .view import View


def _users(ctx, config: dict, now: int) -> None:
    db = ctx.store.conn
    for u in config.get("users", []):
        users.ensure(db, config, u["name"], now, u.get("display_name"), u.get("email"))
    names = [config["default_user"], *(p.get("lead") for p in config.get("projects", []))]
    for issue in config.get("issues", []):
        names += [issue.get("assignee"), issue.get("reporter")]
    for name in filter(None, names):
        users.ensure(db, config, name, now)


def _projects(db, config: dict) -> None:
    for i, p in enumerate(config.get("projects", [])):
        db.execute("INSERT INTO projects(id, key, name, type, lead) VALUES (?, ?, ?, ?, ?)",
                   (10000 + i, p["key"].upper(), p.get("name", p["key"]), p.get("type", "software"), p.get("lead")))


def _agile(db, config: dict, now: int) -> None:
    for b in config.get("boards", []):
        db.execute("INSERT INTO boards(id, name, type, project_id) VALUES (?, ?, ?, ?)",
                   (b["id"], b["name"], b.get("type", "scrum"), records.project(db, b["project"])["id"]))
    for s in config.get("sprints", []):
        db.execute("INSERT INTO sprints(id, board_id, name, state, start_ms, end_ms, complete_ms, created_ms, goal) "
                   "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                   (s["id"], s["board"], s["name"], s.get("state", "future"), times.at(s.get("start"), now),
                    times.at(s.get("end"), now), times.at(s.get("complete"), now), now, s.get("goal", "")))


def _issue(view: View, spec: dict, now: int) -> None:
    project = records.project(view.db, spec["project"])
    values = {"summary": spec["summary"], "issuetype": spec.get("type", "Task"), "priority": spec.get("priority"),
              "assignee": spec.get("assignee"), "reporter": spec.get("reporter"), "labels": spec.get("labels"),
              "description": spec.get("description"), SPRINT_FIELD: spec.get("sprint"), "parent": spec.get("parent")}
    created = times.at(spec.get("created"), now) or now
    at = {"status": spec.get("status"), "rank": spec.get("rank"), "created_ms": created,
          "updated_ms": times.at(spec.get("updated"), now) or created}
    rules.insert_issue(view, spec.get("reporter") or view.config["default_user"], project, values, at)


def seed(ctx, config: dict) -> None:
    now, db = ctx.now_ms(), ctx.store.conn
    _users(ctx, config, now)
    _projects(db, config)
    _agile(db, config, now)
    view = View(ctx)
    for spec in config.get("issues", []):
        _issue(view, spec, now)
    del ctx.notices[:]
