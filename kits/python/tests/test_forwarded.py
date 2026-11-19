from arena_part.forwarded import public_base


def test_public_base_reads_forwarded_headers():
    headers = {"X-Forwarded-Proto": "https", "x-forwarded-host": "jira.acme.test"}
    assert public_base(headers, "http://127.0.0.1:9") == "https://jira.acme.test"


def test_public_base_falls_back_without_both_headers():
    assert public_base({"X-Forwarded-Host": "jira.acme.test"}, "http://127.0.0.1:9") == "http://127.0.0.1:9"
    assert public_base({}, "http://127.0.0.1:9") == "http://127.0.0.1:9"
