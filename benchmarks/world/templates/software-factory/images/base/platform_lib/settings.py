"""A service's settings, all from its environment."""
import os
from dataclasses import dataclass

from .version import image_sha


@dataclass(frozen=True)
class Settings:
    service: str
    environment: str
    database_url: str
    db_schema: str
    otlp_endpoint: str
    sentry_dsn: str
    sha: str
    pool_size: int

    @staticmethod
    def from_env(service: str, env=os.environ) -> "Settings":
        return Settings(service=service, environment=env.get("DEPLOY_ENV", "development"),
                        database_url=env.get("DATABASE_URL", ""), db_schema=env.get("DB_SCHEMA", service),
                        otlp_endpoint=env.get("OTEL_EXPORTER_OTLP_ENDPOINT", ""),
                        sentry_dsn=env.get("SENTRY_DSN", ""), sha=image_sha(env),
                        pool_size=int(env.get("DB_POOL_SIZE", "5")))
