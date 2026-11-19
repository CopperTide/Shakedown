"""Boards and sprints as the agile API answers them."""
from . import records, times


def board(view, row) -> dict:
    project = records.project_by_id(view.db, row["project_id"])
    return {"id": row["id"], "self": view.urls.agile(f"board/{row['id']}"), "name": row["name"], "type": row["type"],
            "location": {"projectId": project["id"], "displayName": f"{project['name']} ({project['key']})",
                         "projectName": project["name"], "projectKey": project["key"],
                         "projectTypeKey": project["type"], "avatarURI": f"/secure/projectavatar?avatarId={project['id']}",
                         "name": f"{project['name']} ({project['key']})"}}


def sprint(view, row) -> dict:
    out = {"id": row["id"], "self": view.urls.agile(f"sprint/{row['id']}"), "state": row["state"], "name": row["name"]}
    for key, col in (("startDate", "start_ms"), ("endDate", "end_ms"), ("completeDate", "complete_ms")):
        if row[col] is not None:
            out[key] = times.stamp(row[col])
    out.update({"createdDate": times.stamp(row["created_ms"]), "originBoardId": row["board_id"], "goal": row["goal"]})
    return out
