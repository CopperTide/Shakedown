//! The embedded migrations: one numbered SQL file per version and stream, its checksum and header.

use std::{
    collections::BTreeSet,
    sync::{LazyLock, Mutex},
};

use sha2::{Digest, Sha256};

use crate::Error;

/// A migration stream: the schemas one service owns, named `^[a-z][a-z_]{0,31}$`. This crate owns
/// [`SHAKEDOWN`]; other repos pass their own streams' [`Files`] to [`load`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stream(pub &'static str);

/// Shakedown's own stream: schema `shakedown`.
pub const SHAKEDOWN: Stream = Stream("shakedown");

impl Stream {
    pub fn as_str(self) -> &'static str {
        self.0
    }

    /// True for a name the history table accepts.
    pub fn is_valid(self) -> bool {
        valid_name(self.0)
    }

    /// The stream named `s` (as read from a database), or None for a name no stream can have.
    /// Each distinct name is kept for the life of the process.
    pub fn named(s: &str) -> Option<Stream> {
        static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
        if !valid_name(s) {
            return None;
        }
        let mut names = NAMES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(n) = names.get(s) {
            return Some(Stream(n));
        }
        let n: &'static str = Box::leak(s.to_string().into_boxed_str());
        names.insert(n);
        Some(Stream(n))
    }
}

/// `^[a-z][a-z_]{0,31}$`.
fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=32).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter().all(|c| c.is_ascii_lowercase() || *c == b'_')
}

/// One stream's embedded files: (file name, text) in version order, and the earlier checksums
/// each version also accepts (each needs a reviewer).
#[derive(Debug, Clone, Copy)]
pub struct Files {
    pub stream: Stream,
    pub files: &'static [(&'static str, &'static str)],
    pub also_accept: &'static [(u32, &'static [[u8; 32]])],
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

/// Shakedown's stream. Keep in version order; `tests/files.rs` checks the list against
/// `migrations/shakedown/`.
pub const BASE_FILES: Files = Files {
    stream: SHAKEDOWN,
    files: &[
        (
            "0001_baseline.sql",
            include_str!("../migrations/shakedown/0001_baseline.sql"),
        ),
        (
            "0002_orgs.sql",
            include_str!("../migrations/shakedown/0002_orgs.sql"),
        ),
        (
            "0003_validate.sql",
            include_str!("../migrations/shakedown/0003_validate.sql"),
        ),
    ],
    also_accept: &[],
};

/// The identity stream, which moves to the cloud's crate; kept until the base drops its orgs.
const IDENTITY_FILES: Files = Files {
    stream: Stream("identity"),
    files: &[
        (
            "0001_baseline.sql",
            include_str!("../migrations/identity/0001_baseline.sql"),
        ),
        (
            "0002_orgs.sql",
            include_str!("../migrations/identity/0002_orgs.sql"),
        ),
    ],
    also_accept: &[],
};

/// Parses every file of `sets`, sorted by (stream, version). [`Error::BadFile`] names a bad stream
/// name, file name or header.
pub fn load(sets: &[&Files]) -> Result<Vec<Migration>, Error> {
    let mut all = Vec::new();
    for set in sets {
        if !set.stream.is_valid() {
            return Err(Error::BadFile(format!(
                "stream {:?} must match ^[a-z][a-z_]{{0,31}}$",
                set.stream.0
            )));
        }
        for (file, sql) in set.files {
            let also = set
                .also_accept
                .iter()
                .find(|(v, _)| version_and_name(file).is_some_and(|(n, _)| n == *v))
                .map_or(&[][..], |(_, a)| a);
            all.push(parse(set.stream, file, sql, also)?);
        }
    }
    all.sort_by_key(|m| (m.stream, m.version));
    Ok(all)
}

fn embedded(files: &Files) -> Vec<Migration> {
    load(&[files]).expect("the embedded migrations parse (tests/files.rs checks them)")
}

/// Shakedown's stream, parsed.
pub fn base() -> &'static [Migration] {
    static BASE: LazyLock<Vec<Migration>> = LazyLock::new(|| embedded(&BASE_FILES));
    &BASE
}

/// The identity stream, parsed; it leaves this crate with the base's orgs.
pub fn retiring_identity() -> &'static [Migration] {
    static IDENTITY: LazyLock<Vec<Migration>> = LazyLock::new(|| embedded(&IDENTITY_FILES));
    &IDENTITY
}

/// Reads a file's version and name (`NNNN_name.sql`) and its header lines.
pub fn parse(
    stream: Stream,
    file: &'static str,
    sql: &'static str,
    also_accept: &'static [[u8; 32]],
) -> Result<Migration, Error> {
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

/// `identity >= 2` as (Stream("identity"), 2).
fn parse_requires(rest: &'static str) -> Option<(Stream, u32)> {
    let (s, n) = rest.split_once(">=")?;
    let s = Stream(s.trim());
    s.is_valid().then_some(())?;
    Some((s, n.trim().parse().ok()?))
}

/// Lower-case hex of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
