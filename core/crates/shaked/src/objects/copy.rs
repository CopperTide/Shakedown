//! `shaked store copy`: every object under a prefix from one store to another, each checked by
//! sha256 after it lands. An object already at the destination with the right checksum is
//! skipped, so a copy reruns safely. With a database, the saves whose objects all verified are
//! repointed to the destination; a save whose object no longer matches the checksum its row
//! recorded is never copied or repointed.

use std::collections::HashMap;

use anyhow::Result;
use futures_util::{StreamExt, TryStreamExt};
use uuid::Uuid;

use super::{hex, ObjectKey, ObjectRef, ObjectStores, StoreName};
use crate::store::{saves_on, Store};

/// What a copy did; `failed` holds each key that did not verify and why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyReport {
    pub copied: u64,
    pub skipped: u64,
    pub failed: Vec<(String, String)>,
    pub repointed: u64,
}

/// A save row on the source store: its id, its objects' prefix and its recorded checksum.
pub(crate) struct SaveOnStore {
    pub id: Uuid,
    pub prefix: String,
    pub sha256: Option<String>,
}

enum Outcome {
    Copied,
    Skipped,
    Failed(String),
}

/// Copies what is under `prefix` (everything when None) from `from` to `to`; with `repoint`,
/// repoints the saves whose objects all verified.
pub async fn copy(
    stores: &ObjectStores,
    from: &StoreName,
    to: &StoreName,
    prefix: Option<&str>,
    repoint: Option<&Store>,
) -> Result<CopyReport> {
    let rows = match repoint {
        Some(db) => saves_on(db, from.as_str()).await?,
        None => vec![],
    };
    let keys = listed(stores, from, prefix).await?;
    let per_save = objects_per_save(&rows, &keys);
    let (mut report, bad) = copy_all(stores, (from, to), &keys, &rows, &per_save).await;
    if let Some(db) = repoint {
        let good: Vec<Uuid> = (rows.iter())
            .filter(|r| per_save[&r.id] > 0 && !bad.contains(&r.id))
            .map(|r| r.id)
            .collect();
        let (from, to) = (from.as_str(), to.as_str());
        report.repointed = crate::store::repoint_saves(db, &good, from, to).await?;
    }
    Ok(report)
}

/// Copies each of `keys`: the report, and the saves with an object that failed.
async fn copy_all(
    stores: &ObjectStores,
    (from, to): (&StoreName, &StoreName),
    keys: &[ObjectKey],
    rows: &[SaveOnStore],
    per_save: &HashMap<Uuid, usize>,
) -> (CopyReport, Vec<Uuid>) {
    let (mut report, mut bad) = (CopyReport::default(), vec![]);
    for key in keys {
        let save = rows.iter().find(|r| key.as_str().starts_with(&r.prefix));
        // A save's recorded checksum is its one object's.
        let expected = save
            .filter(|s| per_save[&s.id] == 1)
            .and_then(|s| s.sha256.clone());
        match copy_one(stores, from, to, key, expected).await {
            Outcome::Copied => report.copied += 1,
            Outcome::Skipped => report.skipped += 1,
            Outcome::Failed(why) => {
                report.failed.push((key.as_str().to_string(), why));
                bad.extend(save.map(|s| s.id));
            }
        }
    }
    (report, bad)
}

/// The keys of the objects under `prefix` on `from`, sorted.
async fn listed(
    stores: &ObjectStores,
    from: &StoreName,
    prefix: Option<&str>,
) -> Result<Vec<ObjectKey>> {
    let s = stores.store(from)?;
    let under = match prefix {
        Some(p) => Some(ObjectKey::new(p).map_err(|e| anyhow::anyhow!(e))?.path()),
        None => None,
    };
    let metas: Vec<_> = s.list(under.as_ref()).try_collect().await?;
    let mut keys: Vec<ObjectKey> = metas
        .iter()
        .filter_map(|m| ObjectKey::new(m.location.as_ref()).ok())
        .collect();
    keys.sort();
    Ok(keys)
}

/// How many of `keys` each save holds.
fn objects_per_save(rows: &[SaveOnStore], keys: &[ObjectKey]) -> HashMap<Uuid, usize> {
    let count = |r: &SaveOnStore| {
        keys.iter()
            .filter(|k| k.as_str().starts_with(&r.prefix))
            .count()
    };
    rows.iter().map(|r| (r.id, count(r))).collect()
}

/// The sha256 of the object `r` names, read as a stream; None when it is not there.
async fn sha_of(stores: &ObjectStores, r: &ObjectRef) -> Result<Option<String>> {
    let mut stream = match stores.get(r).await {
        Ok(s) => s,
        Err(e) if is_not_found(&e) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    while let Some(chunk) = stream.next().await {
        ctx.update(&chunk?);
    }
    Ok(Some(hex(ctx.finish().as_ref())))
}

fn is_not_found(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<object_store::Error>(),
        Some(object_store::Error::NotFound { .. })
    )
}

/// Copies one object, verified against `expected` (else against the source's own checksum).
async fn copy_one(
    stores: &ObjectStores,
    from: &StoreName,
    to: &StoreName,
    key: &ObjectKey,
    expected: Option<String>,
) -> Outcome {
    match try_copy_one(stores, from, to, key, expected).await {
        Ok(o) => o,
        Err(e) => Outcome::Failed(format!("{e:#}")),
    }
}

async fn try_copy_one(
    stores: &ObjectStores,
    from: &StoreName,
    to: &StoreName,
    key: &ObjectKey,
    expected: Option<String>,
) -> Result<Outcome> {
    let src = ObjectRef {
        store: from.clone(),
        key: key.clone(),
    };
    let dst = ObjectRef {
        store: to.clone(),
        key: key.clone(),
    };
    let source = sha_of(stores, &src).await?.unwrap_or_default();
    let want = expected.unwrap_or_else(|| source.clone());
    if source != want {
        return Ok(Outcome::Failed(
            "the source no longer matches its recorded checksum".into(),
        ));
    }
    if sha_of(stores, &dst).await?.as_deref() == Some(want.as_str()) {
        return Ok(Outcome::Skipped);
    }
    write_verified(stores, &src, &dst, &want).await
}

/// Writes `src`'s object to `dst` and checks it reads back as `want`; a copy that does not is
/// removed.
async fn write_verified(
    stores: &ObjectStores,
    src: &ObjectRef,
    dst: &ObjectRef,
    want: &str,
) -> Result<Outcome> {
    let body = stores.get(src).await?.map_err(std::io::Error::other);
    stores.put_to(&dst.store, &dst.key, body).await?;
    if sha_of(stores, dst).await?.as_deref() == Some(want) {
        return Ok(Outcome::Copied);
    }
    let _ = stores.store(&dst.store)?.delete(&dst.key.path()).await;
    Ok(Outcome::Failed(
        "the copy did not verify; removed it".into(),
    ))
}
