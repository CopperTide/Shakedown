"""`search(query:, type:)`: issues and pull requests by the same query subset as REST search."""
from ...rest import search as rest_search
from ..paging import connection
from .issue import issue_or_pull


def search(v, query: str, type: str, **page) -> dict:
    found = rest_search.find(v, query) if type == "ISSUE" else []
    out = connection(found, lambda rr: issue_or_pull(v, *rr), **page)
    return {**out, "issueCount": len(found) if type == "ISSUE" else 0, "repositoryCount": 0, "userCount": 0,
            "discussionCount": 0, "codeCount": 0, "wikiCount": 0}
