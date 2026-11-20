#[tokio::main]
async fn main() -> anyhow::Result<()> {
    shakedown_part::serve(shakedown_service::Service::live_from_env()).await
}
