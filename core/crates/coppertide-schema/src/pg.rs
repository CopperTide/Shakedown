//! The migrator's [`Conn`] for a tokio-postgres client (shaked's driver).

use crate::{migrator::Applied, Conn, Error, Stream};

fn sql_error(e: tokio_postgres::Error) -> Error {
    match e.as_db_error() {
        Some(db) => Error::Sql(format!("{} ({})", db.message(), db.code().code())),
        None => Error::Sql(e.to_string()),
    }
}

impl Conn for tokio_postgres::Client {
    async fn batch(&mut self, sql: &str) -> Result<(), Error> {
        self.batch_execute(sql).await.map_err(sql_error)
    }

    async fn applied(&mut self) -> Result<Vec<Applied>, Error> {
        let exists: bool = self
            .query_one(
                "SELECT to_regclass('schema_meta.migrations') IS NOT NULL",
                &[],
            )
            .await
            .map_err(sql_error)?
            .get(0);
        if !exists {
            return Ok(Vec::new());
        }
        let rows = self
            .query("SELECT stream, version, checksum FROM schema_meta.migrations ORDER BY stream, version", &[])
            .await
            .map_err(sql_error)?;
        rows.iter()
            .map(|r| {
                let s: String = r.get(0);
                let version: i32 = r.get(1);
                Ok(Applied {
                    stream: Stream::parse(&s)
                        .ok_or_else(|| Error::Sql(format!("unknown stream {s:?}")))?,
                    version: version as u32,
                    checksum: r.get(2),
                })
            })
            .collect()
    }
}
