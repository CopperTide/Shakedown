"""Users: the config's people and each agent (made on first sight). Cloud names a user by a 24-hex
`accountId`; Server by `name` and `key`. The world's own name for a user is `name` (its login)."""
import hashlib
import re

ACCOUNT_ID = re.compile(r"[0-9a-f]{24}")


def account_id(name: str) -> str:
    return hashlib.sha256(f"jira-user:{name}".encode()).hexdigest()[:24]


def email_of(config: dict, name: str) -> str:
    return f"{name}@{config.get('email_domain', 'example.test')}"


def by_name(db, name: str):
    return db.execute("SELECT * FROM users WHERE name = ?", (name,)).fetchone()


def ensure(db, config: dict, name: str, now_ms: int, display_name: str | None = None, email: str | None = None):
    """The user called `name`, made now when it is not there yet."""
    row = by_name(db, name)
    if row is None:
        db.execute("INSERT INTO users(name, account_id, display_name, email, created_ms) VALUES (?, ?, ?, ?, ?)",
                   (name, account_id(name), display_name or name, email or email_of(config, name), now_ms))
        row = by_name(db, name)
    return row


def find(db, value: str):
    """The user a request names by accountId, name, key or email."""
    if not value:
        return None
    return db.execute("SELECT * FROM users WHERE account_id = ? OR name = ? OR lower(email) = lower(?)",
                      (value, value, value)).fetchone()


def find_in(db, ref) -> tuple[bool, object]:
    """`(given, user)` for a request's user object (`{accountId}`, `{name}`, `{key}`, `{id}`, `{emailAddress}`)
    or bare id; `given` is False for null or `-1` (nobody)."""
    if isinstance(ref, dict):
        ref = next((ref[k] for k in ("accountId", "name", "key", "id", "emailAddress") if ref.get(k) is not None), None)
    if ref in (None, "", "-1"):
        return False, None
    return True, find(db, str(ref))


def search(db, text: str) -> list:
    """Users whose accountId is `text`, or whose name, display name (or a word of it) or email starts
    with it (any case), by name. The jira client finds an assignee by searching for its accountId."""
    like = f"{text.lower()}%"
    return db.execute("SELECT * FROM users WHERE account_id = ? OR lower(name) LIKE ? OR lower(display_name) LIKE ? "
                      "OR lower(email) LIKE ? OR lower(display_name) LIKE ? ORDER BY name",
                      (text, like, like, like, f"% {like}")).fetchall()


def shape(urls, cloud: bool, row, full: bool = True) -> dict:
    """A user as Jira answers it (`UserDetails` when not `full`)."""
    out = {"self": urls.user(cloud, row)}
    if cloud:
        out.update({"accountId": row["account_id"], "accountType": "atlassian"})
    else:
        out.update({"name": row["name"], "key": row["name"]})
    out.update({"emailAddress": row["email"], "avatarUrls": urls.avatars("user", row["id"]),
                "displayName": row["display_name"], "active": True, "timeZone": "UTC"})
    if full:
        out["locale"] = "en_US"
    return out
