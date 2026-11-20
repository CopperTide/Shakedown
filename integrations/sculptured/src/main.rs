#[tokio::main]
async fn main() -> anyhow::Result<()> {
    shakedown_part::serve(shakedown_sculptured::Sculptured::from_env()).await
}
