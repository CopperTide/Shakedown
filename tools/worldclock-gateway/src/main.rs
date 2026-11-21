//! The world-time gateway program: ingest on `INGEST_ADDR` (default 0.0.0.0:4318), queries on
//! `QUERY_ADDR` (0.0.0.0:4319), the world clock from `CLOCK_FILE` (/shakedown/clock.json); the
//! stores at `TRACES_URL`, `LOGS_URL`, `METRICS_URL`, `PROMETHEUS_URL`, `LOKI_URL`, `TEMPO_URL`.
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use worldclock_gateway::clock::Clock;
use worldclock_gateway::{ingest_router, query_router, Gateway, Upstreams};

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is not set"))
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn upstreams() -> Result<Upstreams> {
    Ok(Upstreams {
        traces: env("TRACES_URL")?,
        logs: env("LOGS_URL")?,
        metrics: env("METRICS_URL")?,
        prometheus: env("PROMETHEUS_URL")?,
        loki: env("LOKI_URL")?,
        tempo: env("TEMPO_URL")?,
    })
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let clock = Arc::new(Clock::of_file(PathBuf::from(env_or(
        "CLOCK_FILE",
        "/shakedown/clock.json",
    ))));
    clock.clone().poll();
    let g = Arc::new(Gateway::new(clock, upstreams()?));
    let ingest = tokio::net::TcpListener::bind(env_or("INGEST_ADDR", "0.0.0.0:4318")).await?;
    let query = tokio::net::TcpListener::bind(env_or("QUERY_ADDR", "0.0.0.0:4319")).await?;
    let (i, q) = (ingest_router(g.clone()), query_router(g));
    eprintln!(
        "worldclock-gateway: ingest on {}, queries on {}",
        ingest.local_addr()?,
        query.local_addr()?
    );
    tokio::try_join!(axum::serve(ingest, i), axum::serve(query, q))?;
    Ok(())
}
