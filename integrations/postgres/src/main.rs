#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut p = shakedown_postgres::Postgres::from_env();
    if std::env::args().any(|a| a == "--sql-tools") {
        p = p.with_sql_tools();
    }
    shakedown_part::serve(p).await
}
