"""Every absolute URL the twin answers with, from the request's public base (never its upstream)."""
from dataclasses import dataclass

API = "/api/v3"


@dataclass(frozen=True)
class Urls:
    base: str

    def api(self, path: str = "") -> str:
        return f"{self.base}{API}{path}"

    def html(self, path: str = "") -> str:
        return f"{self.base}{path}"

    def git(self, owner: str, repo: str) -> str:
        return f"{self.base}/{owner}/{repo}.git"

    def repo_api(self, full_name: str, path: str = "") -> str:
        return self.api(f"/repos/{full_name}{path}")

    def repo_html(self, full_name: str, path: str = "") -> str:
        return self.html(f"/{full_name}{path}")

    @property
    def host(self) -> str:
        return self.base.split("://", 1)[-1]
