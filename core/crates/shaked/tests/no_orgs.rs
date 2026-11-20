//! Open-source Shakedown is for personal use: its engine, schema, UI and SDK name no organisation,
//! tenant or membership and set up no row-level security. A line that must say such a word (a
//! domain name, a comment about the hosting layer) is listed in `no_orgs_allow.txt` as
//! `<path from the repo root>:<the trimmed line>`; a listed line that no longer hits fails too.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// The trees checked, from the repo root.
const TREES: [&str; 5] = [
    "core/crates/shaked/src",
    "core/crates/shakedown-schema/src",
    "core/crates/shakedown-schema/migrations",
    "ui/src",
    "sdk/python/src",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// True when `line` names an org, a tenant, a membership or row-level security.
fn hits(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    let words = l
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| w == "org" || w == "orgs");
    words
        || [
            "org_id",
            "tenant",
            "membership",
            "row level security",
            "set_config('app.",
        ]
        .iter()
        .any(|w| l.contains(w))
}

/// Every hit in the trees, as `<path>:<trimmed line>`.
fn found() -> BTreeSet<String> {
    let root = repo_root();
    let mut out = BTreeSet::new();
    for tree in TREES {
        let mut all = Vec::new();
        files(&root.join(tree), &mut all);
        for f in all {
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let rel = f.strip_prefix(&root).unwrap().to_string_lossy().to_string();
            for line in text.lines().filter(|l| hits(l)) {
                out.insert(format!("{rel}:{}", line.trim()));
            }
        }
    }
    out
}

#[test]
fn the_check_sees_what_it_should() {
    assert!(hits("WHERE org_id = $1"));
    assert!(hits("// each org's runs"));
    assert!(hits("ALTER TABLE t ENABLE ROW LEVEL SECURITY"));
    assert!(hits("SELECT set_config('app.user_id', $1, true)"));
    assert!(!hits("organise the forks; a borg_queue"));
}

#[test]
fn open_source_names_no_orgs() {
    let allowed: BTreeSet<String> = include_str!("no_orgs_allow.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    let found = found();
    let new: Vec<_> = found.difference(&allowed).collect();
    assert!(
        new.is_empty(),
        "open source names orgs:\n{}",
        new.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let stale: Vec<_> = allowed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "allowed lines that no longer hit: {stale:?}"
    );
}
