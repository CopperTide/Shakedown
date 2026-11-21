"""Users (each agent is one, made on first sight) and the user objects REST answers with."""
import base64

from .gitcmd import Person
from .times import iso


def node_id(kind: str, id_: int) -> str:
    return base64.b64encode(f"04:{kind}{id_}".encode()).decode()


def find(db, login: str):
    return db.execute("SELECT * FROM users WHERE login = ?", (login,)).fetchone()


def ensure(db, login: str, now_ms: int, kind: str = "User"):
    """The user named `login`, made now when it is not there yet."""
    row = find(db, login)
    if row is None:
        db.execute("INSERT INTO users(login, type, created_ms) VALUES (?, ?, ?)", (login, kind, now_ms))
        row = find(db, login)
    return row


def simple_user(urls, row) -> dict:
    login, api = row["login"], urls.api(f"/users/{row['login']}")
    return {"login": login, "id": row["id"], "node_id": node_id(row["type"], row["id"]),
            "avatar_url": urls.html(f"/avatars/u/{row['id']}"), "gravatar_id": "",
            "url": api, "html_url": urls.html(f"/{login}"), "followers_url": f"{api}/followers",
            "following_url": f"{api}/following{{/other_user}}", "gists_url": f"{api}/gists{{/gist_id}}",
            "starred_url": f"{api}/starred{{/owner}}{{/repo}}", "subscriptions_url": f"{api}/subscriptions",
            "organizations_url": f"{api}/orgs", "repos_url": f"{api}/repos", "events_url": f"{api}/events{{/privacy}}",
            "received_events_url": f"{api}/received_events", "type": row["type"], "site_admin": False}


def public_user(urls, row) -> dict:
    return {**simple_user(urls, row), "name": row["login"], "company": None, "blog": "", "location": None,
            "email": None, "hireable": None, "bio": None, "public_repos": 0, "public_gists": 0,
            "followers": 0, "following": 0, "created_at": iso(row["created_ms"]), "updated_at": iso(row["created_ms"])}


def private_user(urls, row) -> dict:
    return {**public_user(urls, row), "private_gists": 0, "total_private_repos": 0, "owned_private_repos": 0,
            "disk_usage": 0, "collaborators": 0, "two_factor_authentication": False}


def email(config: dict, login: str) -> str:
    """The address commits made through the twin's API carry for a user (on its first host)."""
    hosts = config.get("hosts") or [{"host": "localhost"}]
    return f"{login}@users.noreply.{hosts[0]['host']}"


def person(config: dict, login: str, at_ms: int) -> Person:
    return Person(login, email(config, login), at_ms)
