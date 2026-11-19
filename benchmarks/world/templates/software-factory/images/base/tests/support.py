"""A meter whose metrics the test can read back, and the names Prometheus' OTLP receiver gives them."""
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import InMemoryMetricReader

from platform_lib.settings import Settings

SETTINGS = Settings(service="orders", environment="production", database_url="", otlp_endpoint="",
                    sentry_dsn="", sha="4f2a9c1", pool_size=5,
                    db_schema="orders")
UNITS = {"s": "seconds", "ms": "milliseconds", "By": "bytes"}


def reading_meter():
    reader = InMemoryMetricReader()
    return MeterProvider(metric_readers=[reader]).get_meter("test"), reader


def collected(reader) -> list:
    data = reader.get_metrics_data()
    return [m for rm in data.resource_metrics for sm in rm.scope_metrics for m in sm.metrics]


def prometheus_names(metric) -> set:
    """Prometheus 3's default OTLP translation (`UnderscoreEscapingWithSuffixes`): dots to underscores, the unit as a
    suffix, `_total` on monotonic sums, `_bucket`/`_count`/`_sum` on classic histograms."""
    name = metric.name.replace(".", "_")
    unit = UNITS.get(metric.unit or "", "")
    if unit and not name.endswith(unit):
        name = f"{name}_{unit}"
    if type(metric.data).__name__ == "Histogram":
        return {f"{name}_bucket", f"{name}_count", f"{name}_sum"}
    if type(metric.data).__name__ == "Sum" and metric.data.is_monotonic:
        return {f"{name}_total"}
    return {name}

