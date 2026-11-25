//! Observability: every run as an OpenTelemetry trace sent to Langfuse, Arize Phoenix or any
//! OTLP/HTTP collector; Prometheus and OTLP metrics; and an OTLP receiver that takes an agent's
//! own spans into its run's record.
//!
//! ```text
//!  record (Postgres) ──► tree ──► encode ──► send ──► target (Langfuse │ Phoenix │ OTLP)
//!     ▲                  spans     protobuf   POST      headers sealed in the vault
//!     │                     ▲
//!  receive ◄── agent's OTel SDK (OTEL_EXPORTER_OTLP_*: /otlp/{branch}/{agent}/v1/traces)
//!
//!  exporter: queue (observability_exports) ─ due when the branch ended ─ send ─ done │ retry
//!  prom:     snapshot of the records ─► /metrics (Prometheus text) │ OTLP metrics push
//! ```

pub mod encode;
pub mod exporter;
pub mod imported;
pub mod inbound;
pub mod prom;
pub mod proto;
pub mod receive;
pub mod send;
pub mod span;
pub mod steps;
pub mod targets;
pub mod tree;
