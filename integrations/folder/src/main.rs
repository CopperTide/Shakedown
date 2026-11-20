#[tokio::main]
async fn main() -> anyhow::Result<()> {
    shakedown_part::serve(shakedown_folder::Folder::from_env()).await
}
