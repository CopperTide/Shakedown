"""A twin may give world tools beside `<SYSTEM>_act` (`world_tools`), run the same way: in one store
transaction, a refusal rolling back the store and the notices it queued."""
import pytest

from arena_part.twin import Done
from notes_twin import NotesTwin

CONFIG = {"hosts": [{"host": "notes.acme.test"}], "default_user": "owner"}
TOOL = {"name": "notes_import", "effect": "write", "description": "Import a note from outside the world.",
        "inputSchema": {"type": "object", "required": ["text"], "properties": {"text": {"type": "string"}}}}


def _import(ctx, args):
    n = ctx.store.conn.execute("INSERT INTO notes(text, by) VALUES (?, ?)", (args.get("text", ""), "import")).lastrowid
    ctx.notify("note_imported", f"note {n} imported", {"id": n})
    return Done(error="empty note") if not args.get("text") else Done({"id": n}, writes=[f"note/{n}"])


class ImportingNotes(NotesTwin):
    def world_tools(self):
        return {"notes_import": (TOOL, _import)}


@pytest.fixture
def notes():
    twin = ImportingNotes()
    twin.start("i", CONFIG, 1, 0)
    yield twin
    twin.stop("i")


def test_extra_tools_are_described_beside_act(notes):
    assert [t["name"] for t in notes.describe()["tools"]] == ["notes_act", "notes_import"]


def test_an_extra_tool_runs_and_its_refusal_changes_nothing(notes):
    assert notes.call_tool("i", "notes_import", {"text": "from outside"}, 0)["result"] == {"id": 1}
    assert notes.call_tool("i", "notes_import", {"text": ""}, 0)["error"] == "empty note"
    assert notes.inspect({"instance": "i"}, [])["docs"] == {"note/1": {"text": "from outside", "by": "import"}}
    assert [e["data"]["kind"] for e in notes.advance("i", 0)["events"]] == ["note_imported"]
    assert notes.call_tool("i", "notes_nothing", {}, 0)["error"] == "unknown tool notes_nothing"
