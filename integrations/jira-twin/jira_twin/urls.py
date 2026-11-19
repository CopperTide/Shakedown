"""Every absolute URL the twin answers with, from the request's public base (never its upstream).
Platform URLs carry the API version the request used (`latest` answers as `2`, as Jira does)."""
from dataclasses import dataclass
from urllib.parse import quote

AGILE = "/rest/agile/1.0"
VERSIONS = ("2", "3", "latest")


@dataclass(frozen=True)
class Urls:
    base: str
    version: str = "2"

    def api(self, path: str) -> str:
        v = "2" if self.version == "latest" else self.version
        return f"{self.base}/rest/api/{v}/{path}"

    def agile(self, path: str) -> str:
        return f"{self.base}{AGILE}/{path}"

    def user(self, cloud: bool, row) -> str:
        if cloud:
            return self.api(f"user?accountId={row['account_id']}")
        return self.api(f"user?username={quote(row['name'])}")

    def avatars(self, kind: str, id_) -> dict:
        return {f"{n}x{n}": f"{self.base}/secure/{kind}avatar?size={n}&avatarId={id_}" for n in (48, 24, 16, 32)}
