"""The one token a Jira twin accepts: a Basic password (`email:token`, as Atlassian's API tokens are
sent) or a Bearer token (Server's personal access tokens). The Basic user does not choose who the
request acts as: the agent the proxy names does."""
import base64
import hmac


def check(headers, token: str) -> str:
    """`ok`, `missing` or `bad`."""
    value = headers.get("authorization", "")
    scheme, _, cred = value.partition(" ")
    if not cred:
        return "missing"
    if scheme.lower() == "basic":
        try:
            cred = base64.b64decode(cred).decode().partition(":")[2]
        except ValueError:
            return "bad"
    elif scheme.lower() != "bearer":
        return "bad"
    return "ok" if hmac.compare_digest(cred.encode(), token.encode()) else "bad"
