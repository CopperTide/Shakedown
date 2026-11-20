#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let p = shakedown_erpnext::Erpnext::from_env()?;
    shakedown_part::serve(p).await
}
