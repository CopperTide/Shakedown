"""The API root, `/meta` (the GHES version gh reads), `/rate_limit` and users."""
from .. import users
from ..headers import RATE_LIMIT
from .api import not_found

ROOT = {
    "current_user_url": "/user", "current_user_authorizations_html_url": "/settings/connections/applications{/client_id}",
    "authorizations_url": "/authorizations", "code_search_url": "/search/code?q={query}{&page,per_page,sort,order}",
    "commit_search_url": "/search/commits?q={query}{&page,per_page,sort,order}", "emails_url": "/user/emails",
    "emojis_url": "/emojis", "events_url": "/events", "feeds_url": "/feeds", "followers_url": "/user/followers",
    "following_url": "/user/following{/target}", "gists_url": "/gists{/gist_id}",
    "issue_search_url": "/search/issues?q={query}{&page,per_page,sort,order}", "issues_url": "/issues",
    "keys_url": "/user/keys", "label_search_url": "/search/labels?q={query}&repository_id={repository_id}{&page,per_page}",
    "notifications_url": "/notifications", "organization_url": "/orgs/{org}",
    "organization_repositories_url": "/orgs/{org}/repos{?type,page,per_page,sort}",
    "organization_teams_url": "/orgs/{org}/teams", "public_gists_url": "/gists/public", "rate_limit_url": "/rate_limit",
    "repository_url": "/repos/{owner}/{repo}", "repository_search_url": "/search/repositories?q={query}{&page,per_page,sort,order}",
    "current_user_repositories_url": "/user/repos{?type,page,per_page,sort}", "starred_url": "/user/starred{/owner}{/repo}",
    "starred_gists_url": "/gists/starred", "user_url": "/users/{user}", "user_organizations_url": "/user/orgs",
    "user_repositories_url": "/users/{user}/repos{?type,page,per_page,sort}",
    "user_search_url": "/search/users?q={query}{&page,per_page,sort,order}",
}
HTML_ROOT = {"current_user_authorizations_html_url"}


def root(call):
    return {k: (call.urls.html(v) if k in HTML_ROOT else call.urls.api(v)) for k, v in ROOT.items()}


def meta(call):
    return {"verifiable_password_authentication": False, "packages": [],
            "installed_version": call.ctx.config.get("installed_version", "3.20.0")}


def _limit(call) -> dict:
    return {"limit": RATE_LIMIT, "remaining": RATE_LIMIT - 1, "reset": call.now() // 1000 + 3600, "used": 1}


def rate_limit(call):
    core = _limit(call)
    return {"resources": {"core": core, "search": _limit(call), "graphql": _limit(call)}, "rate": core}


def me(call):
    return users.private_user(call.urls, call.user)


def user(call):
    row = users.find(call.db, call.req.params["username"])
    if row is None:
        raise not_found()
    return users.public_user(call.urls, row)


def register(api) -> None:
    api.route("GET", "/")(root)
    api.route("GET", "/meta")(meta)
    api.route("GET", "/rate_limit")(rate_limit)
    api.route("GET", "/user")(me)
    api.route("GET", "/users/{username}")(user)
