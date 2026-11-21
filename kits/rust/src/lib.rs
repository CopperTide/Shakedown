//! Kit for writing shakedown integrations and benchmarks (protocol shakedown-part/1).
//!
//! Implement [`Part`] (or [`Benchmark`]) and call [`serve`] (or [`serve_benchmark`]).
mod error;
pub mod helpers;
pub mod proto;
mod serve;
pub mod settings;

pub use error::PartError;
pub use serve::{serve, serve_benchmark, serve_benchmark_on, serve_on, Benchmark, Part};
