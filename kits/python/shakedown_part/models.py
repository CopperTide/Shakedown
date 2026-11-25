"""Model calls of a part or of a benchmark's scoring, through shaked's gateway (docs/protocol.md, "World model
calls").

shaked gives `models: {base_url, token}` at `start` (each instance of a branch) and with `score`. A call goes to
`POST {base_url}/chat/completions`, OpenAI's chat shape, with the token as its bearer: shaked records it, pays it
with the instance's stored provider keys and charges it to the run. Standard library only.
"""
import json
import time
import urllib.error
import urllib.request

#: Names the provider a call goes to (absent: the default provider).
PROVIDER_HEADER = "X-Shakedown-Provider"
#: Names who speaks, for a part that plays several people: the call is recorded as `world.<part>.<actor>`.
ACTOR_HEADER = "X-Shakedown-Actor"

#: Answers worth trying again: rate limits, overload, and a provider or gateway that failed for now.
RETRY_STATUSES = frozenset({429, 500, 502, 503, 504, 529})
#: The longest wait before a retry, whatever `Retry-After` asks.
MAX_WAIT_S = 20.0


class ModelError(Exception):
    """A model call that failed: `status` is the gateway's answer (None when it could not be reached), `body`
    what it said."""

    def __init__(self, message, status=None, body=None):
        super().__init__(message)
        self.status = status
        self.body = body


class _Retry(Exception):
    """A failure worth trying again, and the wait the gateway asked for (seconds), if any."""

    def __init__(self, error, wait=None):
        super().__init__(str(error))
        self.error = error
        self.wait = wait


class Models:
    """A client of the model endpoint shaked gave: `chat` sends one call, tried again a few times with backoff on
    429, 5xx and connection errors."""

    def __init__(self, base_url, token, retries=3, backoff_s=1.0, timeout_s=300.0):
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.retries = retries
        self.backoff_s = backoff_s
        self.timeout_s = timeout_s

    @classmethod
    def of(cls, given):
        """The client of `models` params as shaked sends them, or None when there are none."""
        if isinstance(given, dict) and given.get("base_url") and given.get("token"):
            return cls(given["base_url"], given["token"])
        return None

    def chat(self, messages, model, tools=None, provider=None, actor=None, **fields):
        """The provider's answer (OpenAI's chat completion shape) to `messages` for `model`, with `tools` and any
        other request `fields` (`temperature`, `max_tokens`, ...). `provider` names the provider (else the default
        one), `actor` who speaks. Raises `ModelError` once the call failed for good."""
        body = {"model": model, "messages": messages, **fields}
        if tools:
            body["tools"] = tools
        headers = self._headers(provider, actor)
        for attempt in range(self.retries + 1):
            try:
                return self._post(body, headers)
            except _Retry as r:
                if attempt == self.retries:
                    raise r.error from None
                time.sleep(self._wait(attempt, r.wait))
        raise AssertionError("unreachable")

    def _headers(self, provider, actor):
        headers = {"Content-Type": "application/json", "Authorization": f"Bearer {self.token}"}
        if provider:
            headers[PROVIDER_HEADER] = provider
        if actor:
            headers[ACTOR_HEADER] = actor
        return headers

    def _wait(self, attempt, asked):
        """Seconds before the next try: what the gateway asked, else twice as long each time."""
        wait = asked if asked is not None else self.backoff_s * (2 ** attempt)
        return min(max(wait, 0.0), MAX_WAIT_S)

    def _post(self, body, headers):
        url = f"{self.base_url}/chat/completions"
        req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=self.timeout_s) as resp:
                return json.loads(resp.read().decode() or "{}")
        except urllib.error.HTTPError as e:
            raise _failed(e) from None
        except (urllib.error.URLError, ConnectionError, TimeoutError) as e:
            raise _Retry(ModelError(f"model call failed: {e}")) from None


def _failed(e):
    """The error of an answer that failed: worth a retry for `RETRY_STATUSES`, else a `ModelError` at once."""
    text = e.read().decode(errors="replace")
    try:
        body = json.loads(text)
    except ValueError:
        body = text
    detail = body.get("error", body) if isinstance(body, dict) else body
    error = ModelError(f"model call failed ({e.code}): {detail}", e.code, body)
    if e.code not in RETRY_STATUSES:
        return error
    return _Retry(error, _retry_after(e.headers.get("Retry-After")))


def _retry_after(value):
    """`Retry-After` in seconds, when it is a number."""
    try:
        return float(value)
    except (TypeError, ValueError):
        return None
