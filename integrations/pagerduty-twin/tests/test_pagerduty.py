import httpx2
import pagerduty
import pytest

from conftest import API_HOST, EVENTS_HOST, HOUR_MS, KEY, START_MS, enqueue, list_incidents, raw, rest


def test_events_v2_dedups_and_resolves(pd):
    first = enqueue(pd, "trigger", "k1")
    assert first == {"status": "success", "message": "Event processed", "dedup_key": "k1"}
    enqueue(pd, "trigger", "k1")
    assert len(list_incidents(pd, statuses=["triggered"])) == 1
    enqueue(pd, "resolve", "k1")
    assert pd.docs()["incident/P1"]["status"] == "resolved"


def test_a_trigger_without_a_dedup_key_gets_one(pd):
    key = enqueue(pd, "trigger")["dedup_key"]
    assert key and list_incidents(pd)[0]["incident_key"] == key


def test_an_invalid_event_is_refused(pd):
    status, _, body = raw(pd, "POST", "/v2/enqueue", {"routing_key": KEY, "event_action": "trigger"},
                          host=EVENTS_HOST, token=None, sender=None)
    assert status == 400 and body["status"] == "invalid event" and body["errors"]
    status, _, body = raw(pd, "POST", "/v2/enqueue", {"routing_key": "0" * 32, "event_action": "trigger",
                                                      "payload": {"summary": "s", "source": "x", "severity": "error"}},
                          host=EVENTS_HOST, token=None, sender=None)
    assert status == 400 and body["status"] == "invalid event"


def test_a_trigger_pages_the_on_call_and_is_a_notice(pd):
    pd.notices()
    enqueue(pd, "trigger", "k1")
    incident = list_incidents(pd)[0]
    assert incident["assignments"][0]["assignee"]["summary"] == "Dev Two"
    assert incident["service"]["summary"] == "checkout" and incident["created_at"] == "2026-03-10T06:00:00Z"
    assert incident["self"] == f"https://{API_HOST}/incidents/{incident['id']}"
    assert pd.notices() == [{"system": "pagerduty", "kind": "incident_triggered", "data": {
        "incident": "P1", "service": "checkout", "assignee": "dev", "summary": "Checkout 5xx above 2%"}}]


def test_an_update_without_from_is_refused(pd):
    enqueue(pd, "trigger", "k1")
    status, _, body = raw(pd, "PUT", "/incidents/P1", {"incident": {"type": "incident_reference", "status": "acknowledged"}},
                          sender=None)
    assert status == 400
    assert body == {"error": {"message": "You must specify a user's email address in the \"From\" header", "code": 2001}}
    assert pd.docs()["incident/P1"]["status"] == "triggered"


def test_acknowledge_resolve_and_notes(pd):
    enqueue(pd, "trigger", "k1")
    acked = rest(pd, "PUT", "/incidents/P1", {"incident": {"type": "incident_reference", "status": "acknowledged"}},
                 sender="dev@acme.test")["incident"]
    assert acked["status"] == "acknowledged" and acked["acknowledgements"][0]["acknowledger"]["summary"] == "Dev Two"
    note = rest(pd, "POST", "/incidents/P1/notes", {"note": {"content": "Rolling back"}}, sender="dev@acme.test")["note"]
    assert note["content"] == "Rolling back" and note["user"]["summary"] == "Dev Two"
    assert [n["content"] for n in rest(pd, "GET", "/incidents/P1/notes")["notes"]] == ["Rolling back"]
    rest(pd, "PUT", "/incidents/P1", {"incident": {"type": "incident_reference", "status": "resolved"}})
    assert pd.docs()["incident/P1"] == {**pd.docs()["incident/P1"], "status": "resolved", "service": "checkout",
                                        "created_ms": START_MS}


def test_filters_on_status_service_and_time(pd):
    enqueue(pd, "trigger", "k1")
    pd.part._wall.ms += 2 * HOUR_MS
    enqueue(pd, "trigger", "k2", key="f" * 32)
    search_id = rest(pd, "GET", "/services?query=search")["services"][0]["id"]
    assert [i["incident_key"] for i in list_incidents(pd, service_ids=[search_id])] == ["k2"]
    assert [i["incident_key"] for i in list_incidents(pd, since="2026-03-10T07:00:00Z")] == ["k2"]
    assert [i["incident_key"] for i in list_incidents(pd, until="2026-03-10T07:00:00Z")] == ["k1"]
    assert list_incidents(pd, statuses=["acknowledged"]) == []
    page = rest(pd, "GET", "/incidents?limit=1&total=true")
    assert (page["limit"], page["offset"], page["more"], page["total"], len(page["incidents"])) == (1, 0, True, 2, 1)


def test_on_call_follows_the_rotation_in_world_time(pd):
    assert [o["user"]["summary"] for o in rest(pd, "GET", "/oncalls")["oncalls"]][:1] == ["Dev Two"]
    pd.part._wall.ms += 12 * HOUR_MS
    oncall = rest(pd, "GET", "/oncalls")["oncalls"][0]
    assert oncall["user"]["summary"] == "Ops One" and oncall["escalation_level"] == 1
    assert oncall["start"] == "2026-03-10T17:00:00Z" and oncall["end"] == "2026-03-11T05:00:00Z"


def test_users_me_services_schedules(pd):
    assert rest(pd, "GET", "/users/me", as_user="lead")["user"]["name"] == "Lead Three"
    assert [u["email"] for u in rest(pd, "GET", "/users")["users"]] == ["dev@acme.test", "lead@acme.test", "ops@acme.test"]
    assert [s["name"] for s in rest(pd, "GET", "/services")["services"]] == ["checkout", "search"]
    schedule = rest(pd, "GET", "/schedules")["schedules"][0]
    assert schedule["name"] == "Primary" and [u["summary"] for u in schedule["users"]] == ["Dev Two", "Ops One"]


def test_auth_and_not_found(pd):
    status, _, body = raw(pd, "GET", "/incidents", token="wrong")
    assert status == 401 and body["error"]["code"] == 2006
    status, _, body = raw(pd, "GET", "/incidents/PNOPE")
    assert (status, body) == (404, {"error": {"message": "Not Found", "code": 2100}})


def test_the_world_tool_and_acts(pd):
    out = pd.tool("pagerduty_event", {"action": "trigger", "dedup_key": "alert-1", "summary": "Latency high",
                                      "service": "checkout", "severity": "warning"})
    assert out["result"]["incident"] == "P1" and out["writes"] == ["incident/P1"]
    assert pd.tool("pagerduty_act", {"person": "dev", "action": "ack", "args": {"incident": "P1"}})["result"]["status"] == "acknowledged"
    pd.tool("pagerduty_act", {"person": "dev", "action": "note", "args": {"incident": "P1", "text": "Looking"}})
    assert pd.tool("pagerduty_event", {"action": "resolve", "dedup_key": "alert-1", "service": "checkout"})["result"] == \
        {"incident": "P1", "status": "resolved"}
    assert pd.tool("pagerduty_act", {"person": "dev", "action": "resolve", "args": {"incident": "P9"}})["error"]
    assert pd.tool("pagerduty_event", {"action": "trigger", "summary": "x", "service": "nope"})["error"]
    assert rest(pd, "GET", "/incidents/P1/notes")["notes"][0]["content"] == "Looking"


def test_a_read_only_agent_reads_but_cannot_acknowledge(pd):
    enqueue(pd, "trigger", "k1")
    assert pd.part.http_routes("i")["routes"][0]["honours_readonly"] is True
    ro = {"X-Arena-Readonly": "1"}
    status, headers, _ = raw(pd, "GET", "/incidents", as_user="auditor", headers=ro)
    assert status == 200 and headers["x-arena-effect"] == "read"
    status, headers, body = raw(pd, "PUT", "/incidents/P1", {"incident": {"type": "incident_reference",
                                                                          "status": "acknowledged"}}, headers=ro)
    assert (status, body["error"]["code"], headers["x-arena-effect"]) == (403, 2010, "read")
    status, _, _ = raw(pd, "POST", "/v2/enqueue", {"routing_key": KEY, "event_action": "resolve", "dedup_key": "k1"},
                       host=EVENTS_HOST, token=None, sender=None, headers=ro)
    assert status == 403 and pd.docs()["incident/P1"]["status"] == "triggered"


class _Proxy(httpx2.BaseTransport):
    """Stands in for arenad's proxy: the client talks to the public host, the twin gets X-Forwarded-*."""

    def __init__(self, upstream: str):
        self.upstream, self.inner = httpx2.URL(upstream), httpx2.HTTPTransport()

    def handle_request(self, request):
        host = request.url.host
        request.headers["X-Forwarded-Proto"], request.headers["X-Forwarded-Host"] = request.url.scheme, host
        request.url = request.url.copy_with(scheme=self.upstream.scheme, host=self.upstream.host, port=self.upstream.port)
        request.headers["Host"] = f"{self.upstream.host}:{self.upstream.port}"
        return self.inner.handle_request(request)


@pytest.fixture
def client(pd):
    c = pagerduty.RestApiV2Client(pd.token, default_from="dev@acme.test", transport=_Proxy(pd.up))
    c.url = f"https://{API_HOST}"
    yield c
    c.close()


def test_the_pagerduty_client_lists_and_acknowledges(pd, client):
    enqueue(pd, "trigger", "k1")
    enqueue(pd, "trigger", "k2")
    incidents = client.list_all("incidents", params={"statuses[]": ["triggered"]})
    assert [i["incident_key"] for i in incidents] == ["k1", "k2"]
    updated = client.rput(incidents[0], json={"type": "incident_reference", "status": "acknowledged"})
    assert updated["status"] == "acknowledged"
    client.rpost(f"incidents/{incidents[0]['id']}/notes", json={"content": "Rolling back"})
    assert [n["content"] for n in client.rget(f"incidents/{incidents[0]['id']}/notes")] == ["Rolling back"]
    assert client.rget("users/me")["email"] == "ops@acme.test"
    assert [o["user"]["summary"] for o in client.list_all("oncalls")][:1] == ["Dev Two"]
    assert pd.docs()["incident/P1"]["status"] == "acknowledged"


def test_alerts_clearing_an_open_incident_is_a_notice(pd):
    enqueue(pd, "trigger", "k1")
    pd.notices()
    enqueue(pd, "resolve", "k1")
    assert pd.notices() == [{"system": "pagerduty", "kind": "alerts_cleared", "data": {
        "incident": "P1", "service": "checkout", "assignee": "dev", "summary": "Checkout 5xx above 2%"}}]
    enqueue(pd, "resolve", "k1")
    enqueue(pd, "trigger", "k2")
    pd.tool("pagerduty_act", {"person": "dev", "action": "resolve", "args": {"incident": "P2"}})
    assert [n["kind"] for n in pd.notices()] == ["incident_triggered"]
