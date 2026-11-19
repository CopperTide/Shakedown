"""Relay connections over a list: `first`/`after`/`last`/`before`, cursors and `pageInfo`."""
import base64
from typing import Callable


def cursor(i: int) -> str:
    return base64.b64encode(f"cursor:v2:{i}".encode()).decode()


def _index(c: str | None, default: int) -> int:
    try:
        return int(base64.b64decode(c or "").decode().rsplit(":", 1)[1])
    except (ValueError, IndexError, UnicodeDecodeError):
        return default


def connection(items: list, make: Callable = lambda x: x, first: int | None = None, after: str | None = None,
               last: int | None = None, before: str | None = None, **_) -> dict:
    """The page of `items` the arguments ask for, each made into a node by `make`."""
    total = len(items)
    start = _index(after, -1) + 1 if after else 0
    end = min(_index(before, total), total) if before else total
    if first is not None:
        end = min(end, start + max(first, 0))
    if last is not None:
        start = max(start, end - max(last, 0))
    nodes = [make(x) for x in items[start:end]]
    return {"totalCount": total, "nodes": nodes,
            "edges": [{"node": n, "cursor": cursor(start + i)} for i, n in enumerate(nodes)],
            "pageInfo": {"hasNextPage": end < total, "hasPreviousPage": start > 0,
                         "startCursor": cursor(start) if nodes else None,
                         "endCursor": cursor(end - 1) if nodes else None}}


def empty() -> dict:
    return connection([])
