"""Users, conversations and messages as the Web API answers them (the shapes of Slack's published
description: a message in a thread lists its replies, a reply names its parent's author)."""
from . import ids, records


def team_id(config: dict) -> str:
    return ids.make("T", config.get("team", {}).get("name", "team"))


def user(config: dict, row) -> dict:
    real = row["real_name"]
    profile = {"real_name": real, "real_name_normalized": real, "display_name": row["name"],
               "display_name_normalized": row["name"], "email": row["email"], "first_name": real.split(" ")[0],
               "last_name": " ".join(real.split(" ")[1:]), "team": team_id(config), "avatar_hash": "",
               "status_text": "", "status_emoji": "", "title": "", "phone": "", "skype": "",
               "image_24": None, "image_48": None, "image_72": None, "fields": None}
    return {"id": row["id"], "team_id": team_id(config), "name": row["name"], "deleted": False, "color": "9f69e7",
            "real_name": real, "tz": "UTC", "tz_label": "Coordinated Universal Time", "tz_offset": 0,
            "profile": profile, "is_admin": False, "is_owner": False, "is_primary_owner": False,
            "is_restricted": False, "is_ultra_restricted": False, "is_bot": False, "is_app_user": False,
            "updated": row["created_ms"] // 1000}


def _about(value: str, creator: str, at_ms: int) -> dict:
    return {"value": value, "creator": creator if value else "", "last_set": at_ms // 1000 if value else 0}


def conversation(db, row) -> dict:
    members = db.execute("SELECT COUNT(*) FROM users").fetchone()[0]
    private = bool(row["private"])
    return {"id": row["id"], "name": row["name"], "is_channel": not private, "is_group": private, "is_im": False,
            "is_mpim": False, "is_private": private, "created": row["created_ms"] // 1000, "is_archived": False,
            "is_general": bool(row["general"]), "unlinked": 0, "name_normalized": row["name"], "is_shared": False,
            "is_org_shared": False, "is_ext_shared": False, "is_pending_ext_shared": False, "is_member": True,
            "creator": row["creator"], "topic": _about(row["topic"], row["creator"], row["created_ms"]),
            "purpose": _about(row["purpose"], row["creator"], row["created_ms"]), "previous_names": [],
            "num_members": members}


def _thread(db, row) -> dict:
    replies = records.replies(db, row)
    if not replies:
        return {}
    users = list(dict.fromkeys(r["user"] for r in replies))
    return {"thread_ts": ids.ts(row["ts_us"]), "reply_count": len(replies), "reply_users_count": len(users),
            "latest_reply": ids.ts(replies[-1]["ts_us"]), "reply_users": users, "subscribed": False}


def _reactions(db, row) -> list:
    out: dict = {}
    for r in records.reactions(db, row["id"]):
        out.setdefault(r["name"], []).append(r["user"])
    return [{"name": n, "users": us, "count": len(us)} for n, us in out.items()]


def message(db, config: dict, row, with_reactions: bool = True) -> dict:
    out = {"type": "message", "user": row["user"], "text": row["text"], "ts": ids.ts(row["ts_us"]),
           "team": team_id(config)}
    if row["thread_us"] is not None and row["thread_us"] != row["ts_us"]:
        parent = db.execute("SELECT user FROM messages WHERE ts_us = ?", (row["thread_us"],)).fetchone()
        out.update({"thread_ts": ids.ts(row["thread_us"]), "parent_user_id": parent["user"]})
    else:
        out.update(_thread(db, row))
    reactions = _reactions(db, row) if with_reactions else []
    if reactions:
        out["reactions"] = reactions
    return out
