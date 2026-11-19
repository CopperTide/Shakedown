"""A repo's labels, and the labels on an issue."""
from .. import records
from ..errors import GithubError, invalid, not_found
from . import issue_shapes
from .api import created, no_content, paged
from .issues import the_issue


def _names(body) -> list:
    items = body.get("labels", []) if isinstance(body, dict) else body
    return [x if isinstance(x, str) else x.get("name", "") for x in items or []]


def _body(call):
    try:
        return call.req.json()
    except ValueError:
        return {}


def _on_issue(call, repo, row) -> list:
    return [issue_shapes.label(call, repo, x) for x in records.labels_of(call.db, row["id"])]


def add_labels(call):
    repo, row = the_issue(call)
    records.add_labels(call.db, repo["id"], row["id"], _names(_body(call)))
    return _on_issue(call, repo, row)


def get_issue_labels(call):
    repo, row = the_issue(call)
    return _on_issue(call, repo, row)


def set_issue_labels(call):
    repo, row = the_issue(call)
    records.set_labels(call.db, repo["id"], row["id"], _names(_body(call)))
    return _on_issue(call, repo, row)


def remove_label(call):
    repo, row = the_issue(call)
    label = records.label(call.db, repo["id"], call.req.params["name"])
    if label is None or label["id"] not in {x["id"] for x in records.labels_of(call.db, row["id"])}:
        raise GithubError(404, "Label does not exist")
    call.db.execute("DELETE FROM issue_labels WHERE issue_id = ? AND label_id = ?", (row["id"], label["id"]))
    return _on_issue(call, repo, row)


def create_label(call):
    repo, b = call.repo(), call.body()
    if records.label(call.db, repo["id"], b.get("name", "")) is not None:
        raise invalid("Validation Failed", [{"resource": "Label", "code": "already_exists", "field": "name"}])
    call.db.execute("INSERT INTO labels(repo_id, name, color, description) VALUES (?, ?, ?, ?)",
                    (repo["id"], b.get("name", ""), b.get("color", "ededed").lstrip("#"), b.get("description")))
    return created(issue_shapes.label(call, repo, records.label(call.db, repo["id"], b["name"])))


def list_labels(call):
    repo = call.repo()
    rows = call.db.execute("SELECT * FROM labels WHERE repo_id = ? ORDER BY name", (repo["id"],)).fetchall()
    resp = paged(call, rows)
    resp.body = [issue_shapes.label(call, repo, x) for x in resp.body]
    return resp


def _label(call):
    repo = call.repo()
    row = records.label(call.db, repo["id"], call.req.params["name"])
    if row is None:
        raise not_found()
    return repo, row


def update_label(call):
    repo, row = _label(call)
    b = call.body()
    name = b.get("new_name") or b.get("name") or row["name"]
    call.db.execute("UPDATE labels SET name = ?, color = ?, description = ? WHERE id = ?",
                    (name, b.get("color", row["color"]).lstrip("#"), b.get("description", row["description"]), row["id"]))
    return issue_shapes.label(call, repo, records.label(call.db, repo["id"], name))


def delete_label(call):
    _, row = _label(call)
    call.db.execute("DELETE FROM labels WHERE id = ?", (row["id"],))
    return no_content()


def register(api) -> None:
    i, p = "/repos/{owner}/{repo}/issues/{issue_number}/labels", "/repos/{owner}/{repo}/labels"
    api.route("POST", i)(add_labels)
    api.route("GET", i)(get_issue_labels)
    api.route("PUT", i)(set_issue_labels)
    api.route("DELETE", i + "/{name}")(remove_label)
    api.route("POST", p)(create_label)
    api.route("GET", p)(list_labels)
    api.route("PATCH", p + "/{name}")(update_label)
    api.route("DELETE", p + "/{name}")(delete_label)
