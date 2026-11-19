"""Traces, metrics and logs over OTLP/HTTP to `OTEL_EXPORTER_OTLP_ENDPOINT` (the platform's collector), stamped
with wall time (the platform's gateway restamps them to world time). FastAPI and httpx are traced; their own
metrics are off, since `metrics.py` names the metrics a service emits. Without an endpoint nothing is exported."""
import logging

from opentelemetry import metrics as otel_metrics
from opentelemetry.exporter.otlp.proto.http._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.http.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.httpx import HTTPXClientInstrumentor
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

from .settings import Settings

EXPORT_EVERY_MS = 15_000


class Telemetry:
    def __init__(self, settings: Settings):
        self.enabled = bool(settings.otlp_endpoint)
        resource = Resource.create({"service.name": settings.service, "service.version": settings.sha,
                                    "deployment.environment.name": settings.environment})
        readers = [PeriodicExportingMetricReader(OTLPMetricExporter(), EXPORT_EVERY_MS)] if self.enabled else []
        self.meter_provider = MeterProvider(resource=resource, metric_readers=readers)
        self.tracer_provider = TracerProvider(resource=resource)
        if self.enabled:
            self.tracer_provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter()))
            self._send_logs(resource)

    def meter(self):
        return self.meter_provider.get_meter("platform_lib")

    def instrument(self, app) -> None:
        if not self.enabled:
            return
        quiet = otel_metrics.NoOpMeterProvider()
        FastAPIInstrumentor.instrument_app(app, tracer_provider=self.tracer_provider, meter_provider=quiet,
                                           excluded_urls="healthz")
        HTTPXClientInstrumentor().instrument(tracer_provider=self.tracer_provider, meter_provider=quiet)

    def shutdown(self) -> None:
        self.meter_provider.shutdown()
        self.tracer_provider.shutdown()

    def _send_logs(self, resource: Resource) -> None:
        provider = LoggerProvider(resource=resource)
        provider.add_log_record_processor(BatchLogRecordProcessor(OTLPLogExporter()))
        logging.getLogger().addHandler(LoggingHandler(logger_provider=provider))
