"""`search.messages`: every word of the query in the text (any case), with `in:#channel` and
`from:@name` (or `from:<@U…>`); newest first; Slack's `paging` and `pagination`."""
from . import ids, records, shapes

PAGE = 20


def _terms(query: str) -> tuple[list[str], str | None, str | None]:
    words, channel, author = [], None, None
    for word in query.split():
        low = word.lower()
        if low.startswith("in:"):
            channel = word[3:].lstrip("#").strip("<>").split("|")[0]
        elif low.startswith("from:"):
            author = word[5:].lstrip("@").strip("<>").lstrip("@")
        else:
            words.append(low)
    return words, channel, author


def _wanted(db, row, words, channel, author) -> bool:
    text = row["text"].lower()
    if any(w not in text for w in words):
        return False
    if channel and row["channel_id"] != channel and row["cname"] != channel:
        return False
    return not author or row["user"] == author or records.name_of(db, row["user"]) == author


def _match(db, config: dict, base: str, row) -> dict:
    ch = records.channel(db, row["channel_id"])
    return {"type": "message", "ts": ids.ts(row["ts_us"]), "text": row["text"], "user": row["user"],
            "username": records.name_of(db, row["user"]), "team": shapes.team_id(config),
            "channel": {"id": ch["id"], "name": ch["name"], "is_private": bool(ch["private"])},
            "permalink": f"{base}/archives/{ch['id']}/p{row['ts_us']}"}


def messages(db, config: dict, base: str, query: str, count: int, page: int) -> dict:
    words, channel, author = _terms(query)
    rows = db.execute("SELECT m.*, c.name AS cname FROM messages m JOIN channels c ON c.id = m.channel_id "
                      "ORDER BY m.ts_us DESC").fetchall()
    found = [r for r in rows if _wanted(db, r, words, channel, author)]
    pages = max(1, -(-len(found) // count))
    chunk = found[(page - 1) * count: page * count]
    first = (page - 1) * count + 1 if chunk else 0
    return {"total": len(found), "matches": [_match(db, config, base, r) for r in chunk],
            "paging": {"count": count, "total": len(found), "page": page, "pages": pages},
            "pagination": {"total_count": len(found), "page": page, "per_page": count, "page_count": pages,
                           "first": first, "last": first + len(chunk) - 1 if chunk else 0}}
