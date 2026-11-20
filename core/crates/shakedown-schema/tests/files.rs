//! The migration files themselves, without a database: names, versions, headers and the
//! expand/contract rule.

use std::{collections::BTreeSet, path::Path};

use shakedown_schema::{base, load, Error, Files, Migration, Stream};

/// Every embedded set, one stream each.
fn sets() -> [&'static [Migration]; 1] {
    [base()]
}

/// Every embedded migration.
fn migrations() -> Vec<&'static Migration> {
    sets().into_iter().flatten().collect()
}

/// The `.sql` files in `migrations/<stream>/`, by name.
fn files_on_disk(s: Stream) -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(s.as_str());
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return BTreeSet::new();
    };
    entries
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".sql"))
        .collect()
}

/// (Names are checked by `parse`, which the embedded set went through, and by the round trip to
/// the file names below.)
#[test]
fn every_file_parses_and_versions_are_dense_from_1() {
    for set in sets() {
        let s = set[0].stream;
        assert!(set.iter().all(|m| m.stream == s), "one stream per set");
        let embedded: Vec<_> = set.iter().collect();
        for (i, m) in embedded.iter().enumerate() {
            assert_eq!(
                m.version as usize,
                i + 1,
                "{} versions run 1..=n",
                s.as_str()
            );
        }
        let named: BTreeSet<String> = embedded
            .iter()
            .map(|m| format!("{:04}_{}.sql", m.version, m.name))
            .collect();
        assert_eq!(
            named,
            files_on_disk(s),
            "{}: every file is embedded",
            s.as_str()
        );
    }
}

#[test]
fn every_requires_names_an_existing_migration() {
    for m in migrations() {
        for (s, v) in &m.requires {
            assert!(
                migrations()
                    .iter()
                    .any(|o| o.stream == *s && o.version == *v),
                "{} {} requires {} {v}, which does not exist",
                m.stream.as_str(),
                m.version,
                s.as_str()
            );
        }
    }
}

/// The SQL's words, upper-cased, with `--` comments removed.
fn words(sql: &str) -> Vec<String> {
    let code: String = sql
        .lines()
        .map(|l| l.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    code.to_ascii_uppercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The destructive statements in `w`: DROP (except a constraint the same file adds again, which
/// widens a check), RENAME, SET NOT NULL and a column TYPE change.
fn destructive(w: &[String]) -> Vec<String> {
    let re_added = |name: &str| {
        w.windows(3)
            .any(|x| x[0] == "ADD" && x[1] == "CONSTRAINT" && x[2] == name)
    };
    let mut found = Vec::new();
    for (i, word) in w.iter().enumerate() {
        let next = |k: usize| w.get(i + k).map_or("", String::as_str);
        let hit = match word.as_str() {
            "DROP" => !(next(1) == "CONSTRAINT" && re_added(next(2))),
            "RENAME" => true,
            "SET" => next(1) == "NOT" && next(2) == "NULL",
            "ALTER" if next(1) == "COLUMN" => {
                next(3) == "TYPE" || (next(3) == "SET" && next(4) == "DATA" && next(5) == "TYPE")
            }
            _ => false,
        };
        if hit {
            found.push(w[i..(i + 4).min(w.len())].join(" "));
        }
    }
    found
}

#[test]
fn destructive_statements_carry_a_contract_marker() {
    for m in migrations() {
        let found = destructive(&words(m.sql));
        if !found.is_empty() {
            assert!(
                m.sql.contains("-- contract: unused since "),
                "{} {} ({}) has {found:?} without '-- contract: unused since <sha>'",
                m.stream.as_str(),
                m.version,
                m.name
            );
        }
    }
}

#[test]
fn the_destructive_check_sees_what_it_should() {
    let found = |sql: &str| destructive(&words(sql));
    assert!(!found("ALTER TABLE t DROP COLUMN c").is_empty());
    assert!(!found("ALTER TABLE t ALTER COLUMN c TYPE bigint").is_empty());
    assert!(!found("ALTER TABLE t ALTER COLUMN c SET NOT NULL").is_empty());
    assert!(!found("ALTER TABLE t RENAME TO u").is_empty());
    assert!(!found("ALTER TABLE t ALTER COLUMN c DROP DEFAULT").is_empty());
    assert!(
        found("ALTER TABLE t DROP CONSTRAINT k; ALTER TABLE t ADD CONSTRAINT k CHECK (x)")
            .is_empty()
    );
    assert!(found("CREATE TABLE g (resource_type text) -- DROP in a comment").is_empty());
}

#[test]
fn every_file_sets_no_timeouts_itself() {
    for m in migrations() {
        let w = words(m.sql);
        assert!(
            !w.iter()
                .any(|x| x == "LOCK_TIMEOUT" || x == "STATEMENT_TIMEOUT"),
            "{} {} sets a timeout; the migrator sets them",
            m.stream.as_str(),
            m.version
        );
    }
}

#[test]
fn stream_names_are_checked() {
    let bad = Files {
        stream: Stream("Bad-Name"),
        files: &[],
        also_accept: &[],
    };
    assert!(matches!(load(&[&bad]), Err(Error::BadFile(_))));
    let bad_requires = Files {
        stream: Stream("extra"),
        files: &[("0001_x.sql", "-- requires: Bad-Name >= 1\nSELECT 1;")],
        also_accept: &[],
    };
    assert!(matches!(load(&[&bad_requires]), Err(Error::BadFile(_))));
}
