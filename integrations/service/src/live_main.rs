#[tokio::main]
async fn main() -> anyhow::Result<()> {
    arena_part::serve(arena_service::Service::live_from_env()).await
}
