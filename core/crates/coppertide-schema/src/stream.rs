//! The embedded migrations: one numbered SQL file per version and stream, its checksum and header.

use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::Error;

/// A migration stream: the schemas one service owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stream {
    /// `authz` and `identity`: applied by the Cloud gateway, and by a local shaked.
    Identity,
    /// `cloud`: applied by the Cloud gateway only.
    Cloud,
    /// `shakedown`: applied by shaked.
    Shakedown,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Identity => "identity",
            Stream::Cloud => "cloud",
            Stream::Shakedown => "shakedown",
        }
    }

    pub fn parse(s: &str) -> Option<Stream> {
        [Stream::Identity, Stream::Cloud, Stream::Shakedown]
            .into_iter()
            .find(|x| x.as_str() == s)
    }
}

/// One migration file, as embedded in this binary.
#[derive(Debug, Clone)]
pub struct Migration {
    pub stream: Stream,
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
    /// sha256 of `sql`.
    pub checksum: [u8; 32],
    /// `-- requires: <stream> >= <n>` lines.
    pub requires: Vec<(Stream, u32)>,
    /// `-- no-transaction`: run outside a transaction (e.g. `CREATE INDEX CONCURRENTLY`).
    pub no_transaction: bool,
    /// Checksums of earlier texts of this file that a database may have applied. Each needs a reviewer.
    pub also_accept: &'static [[u8; 32]],
}

impl Migration {
    /// True when `stored` is this file's checksum or one it also accepts.
    pub fn accepts(&self, stored: &[u8]) -> bool {
        stored == self.checksum || self.also_accept.iter().any(|a| stored == a)
    }
}

/// The files: (stream, file name, text). Keep in version order; `tests/files.rs` checks the
/// list against the `migrations/` folders.
const FILES: &[(Stream, &str, &str)] = &[
    (
        Stream::Identity,
        "0001_baseline.sql",
        include_str!("../migrations/identity/0001_baseline.sql"),
    ),
    (
        Stream::Identity,
        "0002_orgs.sql",
        include_str!("../migrations/identity/0002_orgs.sql"),
    ),
    (
        Stream::Shakedown,
        "0001_baseline.sql",
        include_str!("../migrations/shakedown/0001_baseline.sql"),
    ),
    (
        Stream::Shakedown,
        "0002_orgs.sql",
        include_str!("../migrations/shakedown/0002_orgs.sql"),
    ),
];

/// Earlier checksums each (stream, version) also accepts. Empty: no file has changed since it shipped.
const ALSO_ACCEPT: &[(Stream, u32, &[[u8; 32]])] = &[];

/// Every embedded migration, sorted by (stream, version).
pub fn migrations() -> &'static [Migration] {
    static ALL: LazyLock<Vec<Migration>> = LazyLock::new(|| {
        let mut all: Vec<Migration> = FILES
            .iter()
            .map(|(s, file, sql)| parse(*s, file, sql))
            .collect::<Result<_, _>>()
            .expect("the embedded migrations parse (tests/files.rs checks them)");
        all.sort_by_key(|m| (m.stream, m.version));
        all
    });
    &ALL
}

/// Reads a file's version and name (`NNNN_name.sql`) and its header lines.
pub fn parse(stream: Stream, file: &'static str, sql: &'static str) -> Result<Migration, Error> {
    let bad = |why: &str| Error::BadFile(format!("{}/{file}: {why}", stream.as_str()));
    let (version, name) = version_and_name(file).ok_or_else(|| {
        bad("not NNNN_name.sql with a version of 1 or more and a name of [a-z0-9_]+")
    })?;
    let mut requires = Vec::new();
    for line in sql.lines() {
        if let Some(rest) = line.trim().strip_prefix("-- requires:") {
            requires.push(parse_requires(rest).ok_or_else(|| bad("-- requires: <stream> >= <n>"))?);
        }
    }
    let also_accept = ALSO_ACCEPT
        .iter()
        .find(|(s, v, _)| *s == stream && *v == version)
        .map_or(&[][..], |(_, _, a)| a);
    Ok(Migration {
        stream,
        version,
        name,
        sql,
        checksum: Sha256::digest(sql.as_bytes()).into(),
        requires,
        no_transaction: sql.lines().any(|l| l.trim() == "-- no-transaction"),
        also_accept,
    })
}

/// `0002_orgs.sql` as (2, "orgs").
fn version_and_name(file: &'static str) -> Option<(u32, &'static str)> {
    let (num, name) = file.strip_suffix(".sql")?.split_once('_')?;
    let version: u32 = num.parse().ok().filter(|v| *v > 0 && num.len() == 4)?;
    let plain = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_';
    (!name.is_empty() && name.bytes().all(plain)).then_some((version, name))
}

/// `identity >= 2` as (Identity, 2).
fn parse_requires(rest: &str) -> Option<(Stream, u32)> {
    let (s, n) = rest.split_once(">=")?;
    Some((Stream::parse(s.trim())?, n.trim().parse().ok()?))
}

/// Lower-case hex of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
