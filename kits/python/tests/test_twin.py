import pytest

from http_client import http
from arena_part.twin import Response
from notes_twin import NotesTwin

FORWARDED = {"X-Forwarded-Proto": "https", "X-Forwarded-Host": "notes.acme.test"}
CONFIG = {"hosts": [{"host": "notes.acme.test", "env": {"NOTES_URL": "https://notes.acme.test"}}],
          "default_user": "owner"}


class Clock:
    def __init__(self):
        self.ms = 1_773_122_400_000

    def __call__(self):
        return self.ms


@pytest.fixture
def notes():
    twin = NotesTwin(clock=Clock())
    twin.start("i", CONFIG, 1, 0)
    yield twin
    twin.stop("i")


def up(twin, instance="i"):
    return twin.http_routes(instance)["routes"][0]["upstream"]


def test_twin_serves_its_hosts_and_answers_through_the_router(notes):
    routes = notes.http_routes("i")["routes"]
    assert routes[0]["host"] == "notes.acme.test" and routes[0]["upstream"].startswith("http://127.0.0.1:")
    assert routes[0]["env"] == {"NOTES_URL": "https://notes.acme.test"}
    r = http(routes[0]["upstream"], "POST", "/notes", {"text": "hi"}, headers={**FORWARDED, "X-Arena-Agent": "coder"})
    assert r.status == 201
    assert r.json() == {"id": 1, "text": "hi", "by": "coder", "url": "https://notes.acme.test/notes/1"}


def test_without_an_agent_the_default_user_writes_and_urls_fall_back_to_the_upstream(notes):
    r = http(up(notes), "POST", "/notes", {"text": "hi"})
    assert r.json()["by"] == "owner" and r.json()["url"] == f"{up(notes)}/notes/1"


def test_path_params_and_unknown_routes(notes):
    http(up(notes), "POST", "/notes", {"text": "hi"})
    assert http(up(notes), "GET", "/notes/1").json()["text"] == "hi"
    r = http(up(notes), "GET", "/nowhere")
    assert (r.status, r.json()) == (404, {"message": "Not Found"})


def test_a_failed_handler_changes_nothing(notes):   # Review Focus 4
    assert http(up(notes), "POST", "/notes?fail_after_insert=1", {"text": "x"}).status == 500
    assert http(up(notes), "GET", "/notes").json() == []
    assert notes.advance("i", 100) == {"changed": False, "events": []}


def test_a_write_is_saved_and_restored_exactly(notes, saves_root):
    http(up(notes), "POST", "/notes", {"text": "one"})
    out = notes.save("i", "s1", str(saves_root / "notes-twin" / "s1"))
    assert out["exact"] is True and out["bytes"] > 0
    http(up(notes), "POST", "/notes", {"text": "two"})
    notes.start("j", CONFIG, 1, 0, {"save": "s1", "mode": "exact"})
    try:
        assert [n["text"] for n in http(up(notes, "j"), "GET", "/notes").json()] == ["one"]
        assert notes.inspect({"save": "s1"}, [])["docs"] == {"note/1": {"text": "one", "by": "owner"}}
        assert notes.diff({"instance": "i"}, {"instance": "j"}) == {"differing": ["note/2"]}
    finally:
        notes.stop("j")


def test_events_wait_for_the_next_advance(notes):
    out = notes.call_tool("i", "notes_act", {"person": "dana", "action": "add", "args": {"text": "hey"}}, 0)
    assert out["result"] == {"id": 1} and out["writes"] == ["note/1"]
    assert http(up(notes), "GET", "/notes/1").json()["by"] == "dana"
    assert notes.advance("i", 100) == {"changed": True, "events": [{
        "kind": "notice", "text": "dana added note 1",
        "data": {"system": "notes", "kind": "note_added", "data": {"id": 1, "by": "dana"}}}]}
    assert notes.advance("i", 200) == {"changed": False, "events": []}


def test_an_unknown_act_is_refused(notes):
    out = notes.call_tool("i", "notes_act", {"person": "dana", "action": "shred", "args": {}}, 0)
    assert out == {"error": "unknown action shred", "reads": [], "writes": []}


def test_effect_header_is_sent_when_set(notes):
    assert http(up(notes), "GET", "/notes?as=write").headers["x-arena-effect"] == "write"
    assert "x-arena-effect" not in http(up(notes), "GET", "/notes").headers


def test_describe_names_the_act_tool_and_save_points(notes):
    d = notes.describe()
    assert d["restore"] == {"exact": "save", "approximate": False}
    assert (d["uses_clock"], d["save_at"]) == (True, "points")
    assert [(t["name"], t["effect"]) for t in d["tools"]] == [("notes_act", "write")]


def test_world_time_is_used_when_the_part_has_it(notes):
    class Fixed:
        def now_ms(self):
            return 42

    notes.world_time = lambda instance: Fixed()
    assert notes.ctx("i").now_ms() == 42
    del notes.world_time
    assert notes.ctx("i").now_ms() == 1_773_122_400_000


def test_stop_closes_the_server(notes):
    u = up(notes)
    notes.stop("i")
    with pytest.raises(OSError):
        http(u, "GET", "/notes")
    notes.start("i", CONFIG, 1, 0)


def test_a_5xx_answer_rolls_back_and_a_stream_is_sent_in_chunks(notes):
    from notes_twin import router

    @router.route("POST", "/notes/flaky")
    def flaky(req, ctx):
        ctx.store.conn.execute("INSERT INTO notes(text, by) VALUES ('x', 'y')")
        return Response(503, {"message": "try again"})

    @router.route("GET", "/stream")
    def streamed(req, ctx):
        return Response(200, headers={"Content-Type": "text/plain"}, stream=iter([b"ab", b"", b"cd"]))

    try:
        assert http(up(notes), "POST", "/notes/flaky", {}).status == 503
        assert http(up(notes), "GET", "/notes").json() == []
        r = http(up(notes), "GET", "/stream")
        assert (r.body, r.headers["transfer-encoding"]) == (b"abcd", "chunked")
    finally:
        del router.routes[-2:]


def test_a_chunked_request_body_is_read_whole(notes):
    from http.client import HTTPConnection

    conn = HTTPConnection(up(notes).removeprefix("http://"), timeout=10)
    conn.request("POST", "/notes", body=iter([b'{"text": ', b'"chunks"}']), encode_chunked=True,
                 headers={"Transfer-Encoding": "chunked"})
    try:
        assert conn.getresponse().status == 201
    finally:
        conn.close()
    assert http(up(notes), "GET", "/notes/1").json()["text"] == "chunks"


def test_a_path_param_takes_the_rest_of_the_path_even_when_empty():
    from arena_part.twin import Request, Router

    r = Router()
    r.add("GET", "/files/{path:path}", lambda req, ctx: Response(200, req.params))
    req = lambda p: Request("GET", p, {}, {}, b"", None, "http://x")
    assert r.dispatch(req("/files/a/b%2Fc"), None).body == {"path": "a/b/c"}
    assert r.dispatch(req("/files/"), None).body == {"path": ""}


def test_a_request_knows_it_comes_from_a_read_only_agent(notes):
    from notes_twin import router

    @router.route("GET", "/whoami")
    def whoami(req, ctx):
        return Response(200, {"agent": req.agent, "readonly": req.readonly})

    try:
        r = http(up(notes), "GET", "/whoami", headers={"X-Arena-Agent": "auditor", "X-Arena-Readonly": "1"})
        assert r.json() == {"agent": "auditor", "readonly": True}
        assert http(up(notes), "GET", "/whoami").json()["readonly"] is False
    finally:
        del router.routes[-1]


def test_routes_say_when_the_twin_honours_read_only(notes):
    assert "honours_readonly" not in notes.http_routes("i")["routes"][0]
    notes.HONOURS_READONLY = True
    try:
        assert notes.http_routes("i")["routes"][0]["honours_readonly"] is True
    finally:
        del notes.HONOURS_READONLY
