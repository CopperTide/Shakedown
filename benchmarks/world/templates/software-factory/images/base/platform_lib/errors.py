"""Error reports to GlitchTip through the Sentry SDK. Each event is stamped with world time before it is sent,
since GlitchTip shows and sorts events by their own timestamp."""
import sentry_sdk

from .settings import Settings


def world_stamp(clock):
    def before_send(event: dict, hint: dict) -> dict:
        event["timestamp"] = clock.now()
        return event
    return before_send


def setup_errors(settings: Settings, clock) -> None:
    if not settings.sentry_dsn:
        return
    sentry_sdk.init(dsn=settings.sentry_dsn, release=settings.sha, environment=settings.environment,
                    server_name=settings.service, before_send=world_stamp(clock), traces_sample_rate=0.0)
