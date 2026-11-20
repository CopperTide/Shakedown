from shakedown_part.forwarded import public_base


def test_public_base_reads_forwarded_headers():
    headers = {"X-Forwarded-Proto": "https", "x-forwarded-host": "jira.acme.test"}
    assert public_base(headers, "http://127.0.0.1:9") == "https://jira.acme.test"


def test_public_base_falls_back_without_both_headers():
    assert public_base({"X-Forwarded-Host": "jira.acme.test"}, "http://127.0.0.1:9") == "http://127.0.0.1:9"
    assert public_base({}, "http://127.0.0.1:9") == "http://127.0.0.1:9"


def test_agent_of_reads_the_agent_header():
    from shakedown_part.forwarded import AGENT_HEADER, agent_of

    assert AGENT_HEADER == "x-shakedown-agent"
    assert agent_of({"X-Shakedown-Agent": "coder"}) == "coder"
    assert agent_of({"x-shakedown-agent": ""}) is None
    assert agent_of({}) is None


def test_readonly_of_reads_the_readonly_header():
    from shakedown_part.forwarded import READONLY_HEADER, readonly_of

    assert READONLY_HEADER == "x-shakedown-readonly"
    assert readonly_of({"X-Shakedown-Readonly": "1"}) is True
    assert readonly_of({"x-shakedown-readonly": "0"}) is False
    assert readonly_of({}) is False


def test_the_old_header_names_from_a_shaked_before_2026_11_19_are_read():
    from shakedown_part.forwarded import agent_of, readonly_of

    assert agent_of({"X-Arena-Agent": "coder"}) == "coder"
    assert readonly_of({"X-Arena-Readonly": "1"}) is True
    assert agent_of({"X-Shakedown-Agent": "new", "X-Arena-Agent": "old"}) == "new"
