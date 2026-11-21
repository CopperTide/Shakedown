#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let p = shakedown_twenty_crm::TwentyCrm::from_env()?;
    shakedown_part::serve(p).await
}
