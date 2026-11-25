//! The object storage layer: store URLs, keys under a prefix, writes to one store and reads from
//! any, a verified `store copy` that repoints saves, and the start-up probe. On memory and files;
//! the `live_minio_*` test runs MinIO in Docker (removed afterwards).
mod support;
use support::team;

use std::{path::PathBuf, time::Duration};

use bytes::Bytes;
use serde_json::json;
use shaked::{
    objects::{
        copy, keys,
        url::{backends, Backend},
        CopyReport, ObjectKey, ObjectRef, ObjectStores, StoreName,
    },
    store::{NewSave, Store},
};
use support::{app, records, run};

fn none(_: &str) -> Option<String> {
    None
}

/// A folder of its own, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!(
            "shaked-objects-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn url(&self, sub: &str) -> String {
        let p = self.0.join(sub);
        std::fs::create_dir_all(&p).unwrap();
        format!("file://{}", p.display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn name(s: &str) -> StoreName {
    StoreName::new(s).unwrap()
}

#[test]
fn urls_parse_to_the_right_backends() {
    let one = backends("r2://saves?account=acc&prefix=p").unwrap();
    let Backend::S3(s3) = &one[0].1 else {
        panic!("{one:?}")
    };
    assert_eq!(one[0].0.as_str(), "default");
    assert_eq!(
        s3.endpoint.as_deref(),
        Some("https://acc.r2.cloudflarestorage.com")
    );
    assert_eq!(
        (s3.bucket.as_str(), s3.region.as_str(), s3.prefix.as_deref()),
        ("saves", "auto", Some("p"))
    );
    let two = backends("a=s3://b?region=eu-west-1,b=memory://").unwrap();
    let Backend::S3(s3) = &two[0].1 else {
        panic!("{two:?}")
    };
    assert_eq!(
        (s3.region.as_str(), s3.endpoint.as_deref()),
        ("eu-west-1", None)
    );
    assert!(matches!(two[1].1, Backend::Memory));
    assert!(
        matches!(&backends("file:///tmp/x").unwrap()[0].1, Backend::File(p) if p == &PathBuf::from("/tmp/x"))
    );
}

#[test]
fn bad_urls_are_refused_without_their_secrets_and_write_names_a_store() {
    for bad in [
        "s3://AKIA:secret@b?region=r",
        "s3://b?region=r&secret_access_key=x",
        "ftp://x",
        "file://rel",
        "BAD=memory://",
        "a=memory://,a=memory://",
    ] {
        let e = backends(bad).unwrap_err();
        assert!(!e.contains("AKIA") && !e.contains(":secret"), "{e}");
    }
    let picked = ObjectStores::from_spec("a=memory://,b=memory://", Some("b"), none).unwrap();
    assert_eq!(picked.write_store().as_str(), "b");
    assert!(
        ObjectStores::from_spec("a=memory://", Some("z"), none).is_err(),
        "WRITE must name a store"
    );
}

#[test]
fn keys_stay_under_their_prefix() {
    let k = keys::save("p", "pg", "s1").unwrap();
    assert_eq!(k.as_str(), "p/saves/pg/s1/");
    assert_eq!(
        keys::transcript("p", "r", "b", "a").unwrap().as_str(),
        "p/transcripts/r/b/a.jsonl"
    );
    assert_eq!(keys::export("p", "e").unwrap().as_str(), "p/exports/e/");
    assert!(keys::save("", "pg", "s").is_err(), "an empty prefix");
    assert!(keys::save("p", "..", "s").is_err());
    assert!(keys::save("p", "pg", "../x").is_err());
    assert!(ObjectKey::new("/abs/x").is_err() && ObjectKey::new("a/../b").is_err());
    assert_eq!(k.child("part-0").unwrap().as_str(), "p/saves/pg/s1/part-0");
}

const BIG: usize = 9 * 1024 * 1024 + 7;

fn body(n: usize, seed: u8) -> Bytes {
    Bytes::from(
        (0..n)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect::<Vec<u8>>(),
    )
}

fn sha(b: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, b)
        .as_ref()
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

#[test]
fn put_get_roundtrip_with_checksum_on_memory_and_file() {
    run(async {
        let dir = Dir::new("roundtrip");
        for spec in ["memory://".to_string(), dir.url("f")] {
            let stores = ObjectStores::from_spec(&spec, None, none).unwrap();
            for n in [0, 1000, BIG] {
                let key = ObjectKey::new(&format!("local/t/{n}")).unwrap();
                let data = body(n, 3);
                let stored = stores.put_bytes(&key, data.clone()).await.unwrap();
                assert_eq!(
                    (stored.bytes, stored.sha256.as_str()),
                    (n as u64, sha(&data).as_str()),
                    "{spec}"
                );
                assert_eq!(
                    stores.read_all(&stored.r).await.unwrap(),
                    data,
                    "{spec} {n}"
                );
            }
            assert_eq!(
                stores
                    .delete_prefix(stores.write_store(), "local/t/")
                    .await
                    .unwrap(),
                3
            );
        }
    })
}

#[test]
fn reads_come_from_the_store_a_row_names_after_the_write_store_changes() {
    run(async {
        let dir = Dir::new("switch");
        let spec = format!("a={},b={}", dir.url("a"), dir.url("b"));
        let first = ObjectStores::from_spec(&spec, Some("a"), none).unwrap();
        let key = ObjectKey::new("local/x").unwrap();
        let on_a = first
            .put_bytes(&key, Bytes::from_static(b"on a"))
            .await
            .unwrap();
        assert_eq!(on_a.r.store, name("a"));
        let later = ObjectStores::from_spec(&spec, Some("b"), none).unwrap();
        assert_eq!(
            later.read_all(&on_a.r).await.unwrap(),
            Bytes::from_static(b"on a")
        );
        let on_b = later
            .put_bytes(&key, Bytes::from_static(b"on b"))
            .await
            .unwrap();
        assert_eq!(on_b.r.store, name("b"));
        assert_eq!(
            later.read_all(&on_a.r).await.unwrap(),
            Bytes::from_static(b"on a"),
            "a still answers"
        );
    })
}

/// `n` saves of one object each on the write store, with their rows: their object refs.
async fn saves_on(stores: &ObjectStores, store: &Store, n: usize) -> Vec<ObjectRef> {
    let run_id = records(store).create_run("task", &team()).await.unwrap();
    let b = records(store)
        .create_branch(&run_id, None, &team(), &json!([]), None)
        .await
        .unwrap();
    let mut out = vec![];
    for i in 0..n {
        let prefix = keys::save("local", "pg", &format!("s{i}")).unwrap();
        let stored = stores
            .put_bytes(&prefix.child("data").unwrap(), body(1000 + i, i as u8))
            .await
            .unwrap();
        let retention = Duration::from_secs(30 * 86_400);
        let save = NewSave {
            run: &run_id,
            branch: &b,
            integration: "pg",
            save_id: &format!("s{i}"),
            seq: i as i64,
            prefix: &prefix,
            stored: &stored,
            retention,
        };
        records(store).record_save(save).await.unwrap();
        out.push(stored.r);
    }
    out
}

async fn stores_of(store: &Store) -> Vec<String> {
    let rows = records(store).saves().await.unwrap();
    rows.into_iter().map(|s| s.store).collect()
}

/// What a copy did: copied, skipped, failed, repointed.
fn counts(r: &CopyReport) -> (u64, u64, usize, u64) {
    (r.copied, r.skipped, r.failed.len(), r.repointed)
}

/// Copies everything from `a` to `b`, repointing in `store`.
async fn a_to_b(stores: &ObjectStores, store: &Store) -> CopyReport {
    copy(stores, &name("a"), &name("b"), None, Some(store))
        .await
        .unwrap()
}

#[test]
fn copy_verifies_and_repoints() {
    run(async {
        let dir = Dir::new("copy");
        let (store, _clean) = app::meta_store("objects", "copy").await;
        let stores =
            ObjectStores::from_spec(&format!("a=memory://,b={}", dir.url("b")), Some("a"), none)
                .unwrap();
        let refs = saves_on(&stores, &store, 3).await;
        let report = a_to_b(&stores, &store).await;
        assert_eq!(counts(&report), (3, 0, 0, 3), "{report:?}");
        assert_eq!(stores_of(&store).await, ["b", "b", "b"]);
        for r in refs {
            let on_b = ObjectRef {
                store: name("b"),
                key: r.key.clone(),
            };
            assert_eq!(
                stores.read_all(&on_b).await.unwrap(),
                stores.read_all(&r).await.unwrap()
            );
        }
    })
}

#[test]
fn copy_reruns_idempotently_and_never_repoints_a_bad_object() {
    run(async {
        let dir = Dir::new("rerun");
        let (store, _clean) = app::meta_store("objects", "rerun").await;
        let stores = ObjectStores::from_spec(
            &format!("a={},b={}", dir.url("a"), dir.url("b")),
            Some("a"),
            none,
        )
        .unwrap();
        let refs = saves_on(&stores, &store, 3).await;
        // The second save's object no longer matches the checksum its row recorded.
        stores
            .put_bytes(&refs[1].key, body(1001, 99))
            .await
            .unwrap();
        let first = a_to_b(&stores, &store).await;
        assert_eq!(counts(&first), (2, 0, 1, 2), "{first:?}");
        assert_eq!(first.failed[0].0, refs[1].key.as_str());
        let again = a_to_b(&stores, &store).await;
        assert_eq!(counts(&again), (0, 2, 1, 0), "{again:?}");
        let mut at = stores_of(&store).await;
        at.sort();
        assert_eq!(at, ["a", "b", "b"], "the bad save stays where it was");
    })
}

#[test]
fn a_start_that_serves_others_probes_the_write_store() {
    run(async {
        let env = |k: &str| match k {
            "SHAKEDOWN_OBJECT_STORE_REMOTE_ACCESS_KEY_ID" => Some("AKIDTEST".into()),
            "SHAKEDOWN_OBJECT_STORE_REMOTE_SECRET_ACCESS_KEY" => Some("very-secret-value".into()),
            _ => None,
        };
        let spec = "remote=s3://b?region=us-east-1&endpoint=http://127.0.0.1:9&allow_http=true";
        let stores = ObjectStores::from_spec(spec, None, env).unwrap();
        let e = stores.probe().await.unwrap_err();
        assert!(
            e.contains("remote") && !e.contains("very-secret-value") && !e.contains("AKIDTEST"),
            "{e}"
        );
        let fine = ObjectStores::from_spec("memory://", None, none).unwrap();
        fine.probe().await.unwrap();
    })
}

/// MinIO's own images are no longer published; Bitnami's last MinIO build is.
const MINIO_IMAGE: &str = "bitnamilegacy/minio:2025.5.24";

/// A MinIO container on a free port, removed on drop.
struct Minio {
    name: String,
    port: u16,
}

impl Minio {
    fn start() -> Minio {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let name = format!("shaked-minio-{}", uuid::Uuid::new_v4().simple());
        let env = [
            "MINIO_ROOT_USER=shakedown",
            "MINIO_ROOT_PASSWORD=shakedown-test",
        ];
        let ok = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-d",
                "--name",
                &name,
                "-p",
                &format!("127.0.0.1:{port}:9000"),
            ])
            .args(["-e", env[0], "-e", env[1]])
            .arg(MINIO_IMAGE)
            .status()
            .unwrap()
            .success();
        assert!(ok, "docker run {MINIO_IMAGE}");
        Minio { name, port }
    }

    /// `mc` in the container, on the server inside it.
    fn mc(&self, args: &[&str]) -> bool {
        let base = ["exec", &self.name, "mc", "--config-dir", "/tmp/mc"];
        let out = std::process::Command::new("docker")
            .args(base)
            .args(args)
            .output();
        out.is_ok_and(|o| o.status.success())
    }

    /// Makes `bucket` once the server answers (within 90 s).
    async fn ready(&self, bucket: &str) {
        let alias = [
            "alias",
            "set",
            "here",
            "http://127.0.0.1:9000",
            "shakedown",
            "shakedown-test",
        ];
        for _ in 0..180 {
            if self.mc(&alias) && self.mc(&["mb", "--ignore-existing", &format!("here/{bucket}")]) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("MinIO did not make {bucket} in time");
    }
}

impl Drop for Minio {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
    }
}

#[test]
#[ignore]
fn live_minio_s3_put_get_copy() {
    run(async {
        let minio = Minio::start();
        minio.ready("bucket").await;
        let dir = Dir::new("minio");
        let env = |k: &str| match k {
            "SHAKEDOWN_OBJECT_STORE_MINIO_ACCESS_KEY_ID" => Some("shakedown".into()),
            "SHAKEDOWN_OBJECT_STORE_MINIO_SECRET_ACCESS_KEY" => Some("shakedown-test".into()),
            _ => None,
        };
        let spec = format!("minio=s3://bucket?region=us-east-1&endpoint=http://127.0.0.1:{}&allow_http=true,disk={}", minio.port, dir.url("d"));
        let stores = ObjectStores::from_spec(&spec, Some("minio"), env).unwrap();
        let key = ObjectKey::new("local/big").unwrap();
        let data = body(12 * 1024 * 1024, 5);
        let stored = stores.put_bytes(&key, data.clone()).await.unwrap();
        assert_eq!(stored.sha256, sha(&data));
        assert_eq!(stores.read_all(&stored.r).await.unwrap(), data);
        let report = copy(&stores, &name("minio"), &name("disk"), None, None)
            .await
            .unwrap();
        assert_eq!((report.copied, report.failed.len()), (1, 0), "{report:?}");
        let on_disk = ObjectRef {
            store: name("disk"),
            key,
        };
        assert_eq!(stores.read_all(&on_disk).await.unwrap(), data);
    })
}
