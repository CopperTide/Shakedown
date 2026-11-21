//! Object storage for saves, transcripts and exports, over R2, S3, files or memory
//! ([`url`]). Stores are known by name: a row keeps `(store, key)`, never a URL. One store takes
//! the writes (`SHAKEDOWN_OBJECT_STORE_WRITE`, else the first); any registered store serves the
//! reads its rows name, so switching the write store never strands old objects, and
//! `shaked store copy` ([`copy`]) moves them with a checksum check. Nothing is stored here in M1:
//! saves stay on the parts' disks until the sandbox worker uploads them.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use bytes::{Bytes, BytesMut};
use futures_util::{stream::BoxStream, Stream, StreamExt};
use object_store::{
    aws::AmazonS3Builder, local::LocalFileSystem, memory::InMemory, prefix::PrefixStore,
    MultipartUpload, ObjectStore, PutPayload, RetryConfig,
};

pub(crate) mod copy;
pub mod keys;
pub mod url;

pub use copy::{copy, CopyReport};
pub use keys::ObjectKey;
use url::{Backend, S3Spec};

/// A store's name: `^[a-z][a-z0-9-]{0,31}$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StoreName(String);

impl StoreName {
    pub fn new(name: &str) -> Result<StoreName, String> {
        let mut b = name.bytes();
        let first = b.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest = b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
        match first && rest && name.len() <= 32 {
            true => Ok(StoreName(name.to_string())),
            false => Err(format!(
                "{name:?} is not a store name (a-z, 0-9, -; up to 32)"
            )),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a row keeps of an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    pub store: StoreName,
    pub key: ObjectKey,
}

/// An object just written: where, its size and its sha256 (hex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub r: ObjectRef,
    pub bytes: u64,
    pub sha256: String,
}

/// The registered stores and the one that takes writes.
#[derive(Debug)]
pub struct ObjectStores {
    write: StoreName,
    all: BTreeMap<StoreName, Arc<dyn ObjectStore>>,
}

/// Parts of a streamed write: S3's multipart parts are at least 5 MiB but the last.
const PART: usize = 8 * 1024 * 1024;

pub(crate) fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

impl ObjectStores {
    /// The stores `SHAKEDOWN_OBJECT_STORE` names (None when it is unset), the write store from
    /// `SHAKEDOWN_OBJECT_STORE_WRITE`.
    pub fn from_env() -> Result<Option<ObjectStores>, String> {
        let var = |k: &str| {
            parts::settings::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
        };
        let Some(spec) = var("SHAKEDOWN_OBJECT_STORE") else {
            return Ok(None);
        };
        let write = var("SHAKEDOWN_OBJECT_STORE_WRITE");
        ObjectStores::from_spec(&spec, write.as_deref(), var).map(Some)
    }

    /// The stores `spec` names, credentials from `env`; `write` names the write store (else the
    /// first).
    pub fn from_spec(
        spec: &str,
        write: Option<&str>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<ObjectStores, String> {
        let named = url::backends(spec)?;
        let first = named[0].0.clone();
        let mut all = BTreeMap::new();
        for (name, backend) in named {
            let store = build(&name, &backend, &env)?;
            all.insert(name, store);
        }
        let write = match write {
            Some(w) => StoreName::new(w)?,
            None => first,
        };
        if !all.contains_key(&write) {
            return Err(format!(
                "SHAKEDOWN_OBJECT_STORE_WRITE names no store: {}",
                write.as_str()
            ));
        }
        Ok(ObjectStores { write, all })
    }

    pub fn write_store(&self) -> &StoreName {
        &self.write
    }

    pub(crate) fn store(&self, name: &StoreName) -> Result<&Arc<dyn ObjectStore>> {
        (self.all.get(name)).ok_or_else(|| anyhow!("no object store is named {}", name.as_str()))
    }

    /// Writes `body` at `key` on the write store, its sha256 taken on the way.
    pub async fn put(
        &self,
        key: &ObjectKey,
        body: impl Stream<Item = std::io::Result<Bytes>> + Send,
    ) -> Result<Stored> {
        self.put_to(&self.write, key, body).await
    }

    /// [`ObjectStores::put`] of bytes in hand.
    pub async fn put_bytes(&self, key: &ObjectKey, data: Bytes) -> Result<Stored> {
        let one = futures_util::stream::iter([Ok(data)]);
        self.put(key, one).await
    }

    /// Writes `body` at `key` on store `to`.
    pub(crate) async fn put_to(
        &self,
        to: &StoreName,
        key: &ObjectKey,
        body: impl Stream<Item = std::io::Result<Bytes>> + Send,
    ) -> Result<Stored> {
        let store = self.store(to)?;
        let mut w = Writer::new(store.clone(), key.path());
        let mut body = std::pin::pin!(body);
        while let Some(chunk) = body.next().await {
            let chunk = chunk.context("reading what to store")?;
            if let Err(e) = w.push(&chunk).await {
                w.abort().await;
                return Err(e);
            }
        }
        let (bytes, sha256) = w.finish().await?;
        let r = ObjectRef {
            store: to.clone(),
            key: key.clone(),
        };
        Ok(Stored { r, bytes, sha256 })
    }

    /// The object `r` names, from its store (any registered one).
    pub async fn get(
        &self,
        r: &ObjectRef,
    ) -> Result<BoxStream<'static, object_store::Result<Bytes>>> {
        let got = self.store(&r.store)?.get(&r.key.path()).await?;
        Ok(got.into_stream())
    }

    /// The whole object `r` names.
    pub async fn read_all(&self, r: &ObjectRef) -> Result<Bytes> {
        Ok(self
            .store(&r.store)?
            .get(&r.key.path())
            .await?
            .bytes()
            .await?)
    }

    /// Deletes every object under `prefix` on `store`: how many.
    pub async fn delete_prefix(&self, store: &StoreName, prefix: &str) -> Result<u64> {
        let s = self.store(store)?;
        let under = ObjectKey::new(prefix).map_err(|e| anyhow!(e))?.path();
        let listed: Vec<_> = s.list(Some(&under)).collect().await;
        let mut n = 0;
        for meta in listed {
            s.delete(&meta?.location).await?;
            n += 1;
        }
        Ok(n)
    }

    /// Writes, reads back and deletes `_probe/<uuid>` on the write store; the error names the
    /// store (never its URL or keys).
    pub async fn probe(&self) -> Result<(), String> {
        let name = self.write.as_str();
        let failed =
            |e: anyhow::Error| format!("object store {name} does not work: {}", first_line(&e));
        let key = ObjectKey::new(&format!("_probe/{}", uuid::Uuid::new_v4().simple()))
            .map_err(|e| e.to_string())?;
        let data = Bytes::from_static(b"shakedown probe");
        let stored = self.put_bytes(&key, data.clone()).await.map_err(failed)?;
        let back = self.read_all(&stored.r).await.map_err(failed)?;
        let s = self.store(&self.write).map_err(failed)?;
        s.delete(&key.path()).await.map_err(|e| failed(e.into()))?;
        match back == data {
            true => Ok(()),
            false => Err(format!(
                "object store {name} gave back other bytes than it was given"
            )),
        }
    }
}

/// The first line of `e`, without anything that looks like a credential.
fn first_line(e: &anyhow::Error) -> String {
    let text = format!("{e:#}");
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect()
}

/// A streamed write: small objects in one put, larger ones in multipart parts.
struct Writer {
    store: Arc<dyn ObjectStore>,
    path: object_store::path::Path,
    buf: BytesMut,
    upload: Option<Box<dyn MultipartUpload>>,
    sha: ring::digest::Context,
    bytes: u64,
}

impl Writer {
    fn new(store: Arc<dyn ObjectStore>, path: object_store::path::Path) -> Writer {
        Writer {
            store,
            path,
            buf: BytesMut::new(),
            upload: None,
            sha: ring::digest::Context::new(&ring::digest::SHA256),
            bytes: 0,
        }
    }

    async fn push(&mut self, chunk: &[u8]) -> Result<()> {
        self.sha.update(chunk);
        self.bytes += chunk.len() as u64;
        self.buf.extend_from_slice(chunk);
        while self.buf.len() >= PART {
            let part = self.buf.split_to(PART).freeze();
            if self.upload.is_none() {
                self.upload = Some(self.store.put_multipart(&self.path).await?);
            }
            let up = self.upload.as_mut().expect("the upload was just started");
            up.put_part(PutPayload::from(part)).await?;
        }
        Ok(())
    }

    async fn finish(mut self) -> Result<(u64, String)> {
        let rest = self.buf.split().freeze();
        match self.upload.take() {
            None => {
                self.store.put(&self.path, PutPayload::from(rest)).await?;
            }
            Some(mut up) => {
                if !rest.is_empty() {
                    up.put_part(PutPayload::from(rest)).await?;
                }
                up.complete().await?;
            }
        }
        Ok((self.bytes, hex(self.sha.finish().as_ref())))
    }

    async fn abort(&mut self) {
        if let Some(mut up) = self.upload.take() {
            let _ = up.abort().await;
        }
    }
}

/// The store `backend` names, with `name`'s credentials from `env`.
fn build(
    name: &StoreName,
    backend: &Backend,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Arc<dyn ObjectStore>, String> {
    let shown = name.as_str();
    match backend {
        Backend::Memory => Ok(Arc::new(InMemory::new())),
        Backend::File(dir) => {
            std::fs::create_dir_all(dir).map_err(|e| format!("object store {shown}: {e}"))?;
            let fs = LocalFileSystem::new_with_prefix(dir)
                .map_err(|e| format!("object store {shown}: {e}"))?;
            Ok(Arc::new(fs.with_automatic_cleanup(true)))
        }
        Backend::S3(spec) => s3(name, spec, env),
    }
}

/// `name`'s keys: `SHAKEDOWN_OBJECT_STORE_<NAME>_<what>`, else `AWS_<what>`.
fn credential(
    name: &StoreName,
    what: &str,
    env: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let upper = name.as_str().to_ascii_uppercase().replace('-', "_");
    env(&format!("SHAKEDOWN_OBJECT_STORE_{upper}_{what}")).or_else(|| env(&format!("AWS_{what}")))
}

/// `name`'s access key id and secret.
fn keys_of(
    name: &StoreName,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(String, String), String> {
    let id = credential(name, "ACCESS_KEY_ID", env);
    let secret = credential(name, "SECRET_ACCESS_KEY", env);
    id.zip(secret).ok_or_else(|| {
        let upper = name.as_str().to_ascii_uppercase().replace('-', "_");
        format!("object store {} has no keys: set SHAKEDOWN_OBJECT_STORE_{upper}_ACCESS_KEY_ID and _SECRET_ACCESS_KEY", name.as_str())
    })
}

fn s3(
    name: &StoreName,
    spec: &S3Spec,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Arc<dyn ObjectStore>, String> {
    let shown = name.as_str();
    let (id, secret) = keys_of(name, env)?;
    let retry = RetryConfig {
        max_retries: 3,
        retry_timeout: Duration::from_secs(30),
        ..RetryConfig::default()
    };
    let mut b = AmazonS3Builder::new()
        .with_bucket_name(&spec.bucket)
        .with_region(&spec.region)
        .with_access_key_id(id)
        .with_secret_access_key(secret)
        .with_allow_http(spec.allow_http)
        .with_retry(retry);
    if let Some(token) = credential(name, "SESSION_TOKEN", env) {
        b = b.with_token(token);
    }
    if let Some(endpoint) = &spec.endpoint {
        b = b.with_endpoint(endpoint);
    }
    let store: Arc<dyn ObjectStore> = Arc::new(
        b.build()
            .map_err(|e| format!("object store {shown}: {e}"))?,
    );
    Ok(match &spec.prefix {
        Some(p) => Arc::new(PrefixStore::new(store, p.as_str())),
        None => store,
    })
}
