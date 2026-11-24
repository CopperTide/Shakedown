//! Sandbox backends. `live_e2b_…` run on real E2B with `E2B_API_KEY` set and are `#[ignore]`d:
//! `cargo test -p shaked --test hosting -- --ignored live_e2b --nocapture`. Each kills every
//! sandbox it made, also when it fails, and its sandboxes time out after two minutes anyway.

use std::{collections::BTreeMap, time::Duration};

use shaked::hosting::{
    e2b::{E2bBackend, API},
    ProviderRef, ProviderState, SandboxBackend, SandboxSpec, TICKET_PATH,
};

/// What the live check writes and reads back.
const TICKET: &[u8] = b"sdt_live_check\n";

fn live_spec(test: &str) -> SandboxSpec {
    SandboxSpec {
        template: "base".into(),
        allow_out: vec!["example.com".into()],
        timeout: Duration::from_secs(120),
        metadata: BTreeMap::from([("test".into(), test.into())]),
    }
}

/// The sandbox runs, takes a ticket at [`TICKET_PATH`] and gives it back.
async fn write_and_read(e2b: &E2bBackend, sb: &ProviderRef) -> anyhow::Result<()> {
    anyhow::ensure!(
        e2b.state(sb).await? == ProviderState::Running,
        "not running"
    );
    e2b.write_file(sb, TICKET_PATH, TICKET.to_vec()).await?;
    let back = e2b.read_file(sb, TICKET_PATH).await?;
    anyhow::ensure!(
        back == TICKET,
        "read back {:?}",
        String::from_utf8_lossy(&back)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live: creates a real E2B sandbox (needs E2B_API_KEY)"]
async fn live_e2b_create_write_kill() {
    let Ok(key) = std::env::var("E2B_API_KEY") else {
        println!("skipped: E2B_API_KEY is not set");
        return;
    };
    let e2b = E2bBackend::new(key, API.parse().unwrap());
    let sb = e2b
        .create(&live_spec("live_e2b_create_write_kill"))
        .await
        .unwrap();
    println!("created E2B sandbox {}", sb.0);
    let used = write_and_read(&e2b, &sb).await;
    e2b.kill(&sb).await.unwrap();
    println!("killed E2B sandbox {}", sb.0);
    used.unwrap();
    let ticket = String::from_utf8_lossy(TICKET);
    println!("wrote and read back {ticket:?} at {TICKET_PATH}");
    assert_eq!(e2b.state(&sb).await.unwrap(), ProviderState::Gone);
}
