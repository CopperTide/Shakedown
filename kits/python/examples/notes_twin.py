"""Example HTTP twin: notes anyone can post, each by the agent that sent it (see arena_part.twin)."""
from arena_part import serve
from arena_part.twin import Done, HttpTwin, Response, Router

router = Router()


def note(base, row):
    return {"id": row["id"], "text": row["text"], "by": row["by"], "url": f"{base}/notes/{row['id']}"}


@router.route("POST", "/notes")
def create(req, ctx):
    with ctx.store.tx() as db:
        cur = db.execute("INSERT INTO notes(text, by) VALUES (?, ?)", (req.json()["text"], ctx.actor(req)))
        if req.arg("fail_after_insert"):
            raise RuntimeError("failed after the insert, on purpose")
        return Response(201, note(req.base, db.execute("SELECT * FROM notes WHERE id = ?", (cur.lastrowid,)).fetchone()))


@router.route("GET", "/notes")
def index(req, ctx):
    rows = ctx.store.conn.execute("SELECT * FROM notes ORDER BY id").fetchall()
    return Response(200, [note(req.base, r) for r in rows], effect=req.arg("as"))


@router.route("GET", "/notes/{id}")
def show(req, ctx):
    row = ctx.store.conn.execute("SELECT * FROM notes WHERE id = ?", (req.params["id"],)).fetchone()
    return Response(200, note(req.base, row)) if row else router.not_found(req)


class NotesTwin(HttpTwin):
    NAME, SYSTEM = "notes-twin", "notes"
    SCHEMA_SQL = "CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY, text TEXT NOT NULL, by TEXT NOT NULL);"

    def router(self):
        return router

    def docs(self, ctx):
        rows = ctx.store.conn.execute("SELECT * FROM notes").fetchall()
        return {f"note/{r['id']}": {"text": r["text"], "by": r["by"]} for r in rows}

    def act(self, ctx, person, action, args):
        if action != "add":
            return Done(error=f"unknown action {action}")
        n = ctx.store.conn.execute("INSERT INTO notes(text, by) VALUES (?, ?)", (args["text"], person)).lastrowid
        ctx.notify("note_added", f"{person} added note {n}", {"id": n, "by": person})
        return Done({"id": n}, writes=[f"note/{n}"])


if __name__ == "__main__":
    serve(NotesTwin())
