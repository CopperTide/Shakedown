"""Pages of a list and the `Link` header clients follow (`per_page` default 30, at most 100)."""
from urllib.parse import urlencode

DEFAULT, MAX = 30, 100


def _int(value, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _url(req, urls, page: int) -> str:
    query = [(k, v) for k, vs in req.query.items() if k != "page" for v in vs] + [("page", str(page))]
    return f"{urls.html(req.path)}?{urlencode(query)}"


def link_header(req, urls, page: int, last: int) -> str | None:
    rels = []
    if page > 1:
        rels += [("prev", page - 1)]
    if page < last:
        rels += [("next", page + 1), ("last", last)]
    if page > 1:
        rels += [("first", 1)]
    return ", ".join(f'<{_url(req, urls, n)}>; rel="{rel}"' for rel, n in rels) or None


def page(items: list, req, urls) -> tuple[list, str | None]:
    """The page of `items` the request asks for, and its `Link` header (None on a single page)."""
    per = min(max(_int(req.arg("per_page"), DEFAULT), 1), MAX)
    n = max(_int(req.arg("page"), 1), 1)
    last = max((len(items) + per - 1) // per, 1)
    return items[(n - 1) * per:n * per], link_header(req, urls, n, last)
