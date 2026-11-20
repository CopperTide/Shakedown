//! `SHAKEDOWN_OBJECT_STORE`: `name=url,name=url`, or one bare URL (named `default`). A URL never
//! holds a credential (URLs reach logs): keys come from `SHAKEDOWN_OBJECT_STORE_<NAME>_ACCESS_KEY_ID`
//! and `_SECRET_ACCESS_KEY` (else `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`).
//!
//! - `r2://<bucket>?account=<id>[&prefix=p]`: Cloudflare R2 at `https://<id>.r2.cloudflarestorage.com`
//! - `s3://<bucket>?region=<r>[&endpoint=<url>][&allow_http=true][&prefix=p]`: S3 or anything
//!   speaking it (MinIO)
//! - `file:///absolute/dir`, `memory://`

use std::path::PathBuf;

use reqwest::Url;

use super::StoreName;

/// Where an S3-style store is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Spec {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub allow_http: bool,
    pub prefix: Option<String>,
}

/// A store's backend, as its URL names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    S3(S3Spec),
    File(PathBuf),
    Memory,
}

const QUERY_KEYS: [&str; 5] = ["account", "region", "endpoint", "allow_http", "prefix"];

/// Each store `spec` names, in order. Errors name the store, never its URL.
pub fn backends(spec: &str) -> Result<Vec<(StoreName, Backend)>, String> {
    let mut out: Vec<(StoreName, Backend)> = vec![];
    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (name, url) = split_entry(entry);
        let name = StoreName::new(name)?;
        if out.iter().any(|(n, _)| *n == name) {
            return Err(format!("object store {} is named twice", name.as_str()));
        }
        let backend = backend(url).map_err(|e| format!("object store {}: {e}", name.as_str()))?;
        out.push((name, backend));
    }
    match out.is_empty() {
        true => Err("SHAKEDOWN_OBJECT_STORE names no store".into()),
        false => Ok(out),
    }
}

/// `name=url` or a bare URL (`default`).
fn split_entry(entry: &str) -> (&str, &str) {
    let scheme_at = entry.find("://").unwrap_or(entry.len());
    match entry[..scheme_at].find('=') {
        Some(eq) => (&entry[..eq], &entry[eq + 1..]),
        None => ("default", entry),
    }
}

fn backend(url: &str) -> Result<Backend, String> {
    let u = Url::parse(url).map_err(|_| "not a URL".to_string())?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err("a store URL holds no credentials: set SHAKEDOWN_OBJECT_STORE_<NAME>_ACCESS_KEY_ID and _SECRET_ACCESS_KEY".into());
    }
    match u.scheme() {
        "r2" | "s3" => s3_spec(&u).map(Backend::S3),
        "file" if u.host_str().unwrap_or_default().is_empty() && u.path().starts_with('/') => {
            Ok(Backend::File(PathBuf::from(u.path())))
        }
        "file" => Err("file needs an absolute folder (file:///dir)".into()),
        "memory" => Ok(Backend::Memory),
        other => Err(format!("unknown kind {other:?} (r2, s3, file or memory)")),
    }
}

/// An `r2://` or `s3://` URL's store: only the known query settings.
fn s3_spec(u: &Url) -> Result<S3Spec, String> {
    let query: Vec<(String, String)> = u.query_pairs().into_owned().collect();
    if let Some((k, _)) = (query.iter()).find(|(k, _)| !QUERY_KEYS.contains(&k.as_str())) {
        let known = QUERY_KEYS.join(", ");
        return Err(format!("unknown setting {k:?} (known: {known})"));
    }
    let get = |k: &str| query.iter().find(|(q, _)| q == k).map(|(_, v)| v.clone());
    let bucket = (u.host_str())
        .filter(|h| !h.is_empty())
        .ok_or("no bucket")?;
    let (region, endpoint) = match u.scheme() {
        "r2" => {
            let account = get("account").ok_or("r2 needs account=<id>")?;
            let endpoint = format!("https://{account}.r2.cloudflarestorage.com");
            ("auto".to_string(), Some(endpoint))
        }
        _ => (
            get("region").ok_or("s3 needs region=<region>")?,
            get("endpoint"),
        ),
    };
    Ok(S3Spec {
        bucket: bucket.to_string(),
        region,
        endpoint,
        allow_http: u.scheme() == "s3" && get("allow_http").as_deref() == Some("true"),
        prefix: get("prefix"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_url_is_the_default_store_and_names_are_checked() {
        assert_eq!(
            split_entry("s3://b?region=x"),
            ("default", "s3://b?region=x")
        );
        assert_eq!(split_entry("a=s3://b?region=x"), ("a", "s3://b?region=x"));
        assert!(backends("Bad=memory://").is_err());
        let e = backends("x=s3://key:pw@b?region=r").unwrap_err();
        assert!(!e.contains("pw") && e.contains("x"), "{e}");
    }
}
