"""The REST operations the twin serves, each `(METHOD, path)` exactly as Atlassian's descriptions name
it: platform paths under `/rest/api/{2,3}` (and `latest`), agile ones under `/rest/agile/1.0`. The
schema tests check every answer against the descriptions and that the router serves exactly these."""
from ..urls import AGILE

PLATFORM_PREFIX = "/rest/api/{version}"

PLATFORM = [
    ("GET", "/serverInfo"),
    ("GET", "/myself"),
    ("GET", "/user"),
    ("GET", "/user/search"),
    ("GET", "/project"),
    ("GET", "/project/{projectIdOrKey}"),
    ("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes"),
    ("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes/{issueTypeId}"),
    ("GET", "/issuetype"),
    ("GET", "/issuetype/{id}"),
    ("GET", "/status"),
    ("GET", "/status/{idOrName}"),
    ("GET", "/statuscategory/{idOrKey}"),
    ("GET", "/priority"),
    ("GET", "/priority/{id}"),
    ("GET", "/resolution"),
    ("GET", "/resolution/{id}"),
    ("GET", "/issueLinkType"),
    ("GET", "/issueLinkType/{issueLinkTypeId}"),
    ("GET", "/field"),
    ("POST", "/issue"),
    ("GET", "/issue/{issueIdOrKey}"),
    ("PUT", "/issue/{issueIdOrKey}"),
    ("DELETE", "/issue/{issueIdOrKey}"),
    ("GET", "/issue/{issueIdOrKey}/transitions"),
    ("POST", "/issue/{issueIdOrKey}/transitions"),
    ("GET", "/issue/{issueIdOrKey}/comment"),
    ("POST", "/issue/{issueIdOrKey}/comment"),
    ("GET", "/issue/{issueIdOrKey}/comment/{id}"),
    ("PUT", "/issue/{issueIdOrKey}/comment/{id}"),
    ("DELETE", "/issue/{issueIdOrKey}/comment/{id}"),
    ("PUT", "/issue/{issueIdOrKey}/assignee"),
    ("POST", "/issueLink"),
    ("GET", "/issueLink/{linkId}"),
    ("GET", "/search/jql"),
    ("POST", "/search/jql"),
    ("GET", "/search"),
    ("POST", "/search"),
    ("POST", "/search/approximate-count"),
]

AGILE_OPS = [
    ("GET", "/board"),
    ("GET", "/board/{boardId}"),
    ("GET", "/board/{boardId}/sprint"),
    ("GET", "/board/{boardId}/issue"),
    ("GET", "/board/{boardId}/backlog"),
    ("POST", "/sprint"),
    ("GET", "/sprint/{sprintId}"),
    ("PUT", "/sprint/{sprintId}"),
    ("POST", "/sprint/{sprintId}"),
    ("GET", "/sprint/{sprintId}/issue"),
    ("POST", "/sprint/{sprintId}/issue"),
    ("POST", "/backlog/issue"),
    ("PUT", "/issue/rank"),
]

#: `(api, METHOD, path)` with api "platform" or "agile".
COVERED = [("platform", m, p) for m, p in PLATFORM] + [("agile", m, p) for m, p in AGILE_OPS]


def router_ops(router) -> list[tuple[str, str, str]]:
    """The operations a router serves, as in COVERED."""
    out = []
    for r in router.routes:
        if r.pattern.startswith(PLATFORM_PREFIX + "/"):
            out.append(("platform", r.method, r.pattern[len(PLATFORM_PREFIX):]))
        elif r.pattern.startswith(AGILE + "/"):
            out.append(("agile", r.method, r.pattern[len(AGILE):]))
    return out


def documented_path(api: str, path: str, version: str = "2") -> str:
    """The path as the description for `api` (and platform `version`) names it."""
    return f"/rest/api/{version}{path}" if api == "platform" else f"{AGILE}{path}"
