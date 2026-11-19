//! `postgres` volumes: a plain `pg_dump` of the database, made while it runs, and `psql` of it
//! into a new instance's database before the other services start.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use arena_part::helpers::compose::Project;
use arena_part::helpers::folder::{hex, Digest};
use sha2::{Digest as _, Sha256};

use crate::config::VolumeSpec;

/// How long a restored database may take to accept connections.
const READY_WITHIN: Duration = Duration::from_secs(60);

/// Where the dump of `volume` is in the folder `dir`.
pub fn sql_path(dir: &Path, volume: &str) -> PathBuf {
    dir.join(format!("{volume}.sql"))
}

fn service(v: &VolumeSpec) -> Result<&str> {
    match v.service.as_deref() {
        Some(s) => Ok(s),
        None => bail!("volume {} has no database service", v.name),
    }
}

/// Dumps `v`'s database to `dir/<volume>.sql` (the services keep running); the bytes written.
pub fn save(project: &Project, v: &VolumeSpec, dir: &Path) -> Result<u64> {
    let args = ["pg_dump", "-U", v.user(), "--no-owner", v.database()];
    let dump = project.exec(service(v)?, &args)?;
    let path = sql_path(dir, &v.name);
    std::fs::write(&path, &dump)?;
    Ok(dump.len() as u64)
}

/// Waits until `v`'s database accepts connections, then loads `save/<volume>.sql` into it.
pub fn restore(project: &Project, v: &VolumeSpec, save: &Path) -> Result<()> {
    let svc = service(v)?;
    let since = Instant::now();
    while let Err(e) = project.exec(svc, &["pg_isready", "-U", v.user()]) {
        if since.elapsed() > READY_WITHIN {
            bail!(
                "the database of {} was not ready within 60 s: {e:#}",
                v.name
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let args = [
        "psql",
        "-U",
        v.user(),
        "-d",
        v.database(),
        "-q",
        "-v",
        "ON_ERROR_STOP=1",
    ];
    project.exec_in(svc, &args, &sql_path(save, &v.name))
}

/// A dump's digest: SHA-256 of its lines that are no `--` comment (a dump names the time it was
/// made), one file of the dump's size.
pub fn digest(dump: &str) -> Digest {
    let mut h = Sha256::new();
    for line in dump.lines().filter(|l| !l.starts_with("--")) {
        h.update(line.as_bytes());
        h.update(b"\n");
    }
    Digest {
        bytes: dump.len() as u64,
        files: 1,
        hash: hex(&h.finalize()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_digests_ignore_comment_lines() {
        let a = "-- dumped at 10:00\nCREATE TABLE t (n int);\n";
        let b = "-- dumped at 11:30\n--\nCREATE TABLE t (n int);\n";
        assert_eq!(digest(a).hash, digest(b).hash);
        assert_ne!(digest(a).hash, digest("CREATE TABLE u (n int);\n").hash);
        assert_eq!((digest(a).files, digest(a).bytes), (1, a.len() as u64));
    }
}
