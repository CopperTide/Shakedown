//! Object keys: `<prefix>/<kind>/…`, the prefix being the layer's `object_prefix` (`local` with
//! `LocalLayer`). A key never climbs out of its prefix: no `..`, no leading `/`, no empty segment.

use std::fmt;

/// An object's key, or (ending in `/`) the prefix of a group of objects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    pub fn new(key: &str) -> Result<ObjectKey, String> {
        let body = key.strip_suffix('/').unwrap_or(key);
        let bad = |s: &str| s.is_empty() || s == "." || s == ".." || s.contains('\\');
        if body.is_empty() || body.split('/').any(bad) {
            return Err(format!("{key:?} is not a usable object key"));
        }
        Ok(ObjectKey(key.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `name` under this prefix.
    pub fn child(&self, name: &str) -> Result<ObjectKey, String> {
        let base = self.0.strip_suffix('/').unwrap_or(&self.0);
        ObjectKey::new(&format!("{base}/{name}"))
    }

    /// The object-store path (a prefix without its trailing `/`).
    pub fn path(&self) -> object_store::path::Path {
        object_store::path::Path::from(self.0.strip_suffix('/').unwrap_or(&self.0))
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One segment: no `/`, nothing that climbs.
fn segment(s: &str) -> Result<&str, String> {
    match s.is_empty() || s.contains('/') || s == "." || s == ".." {
        true => Err(format!("{s:?} is not a usable key segment")),
        false => Ok(s),
    }
}

/// A world save's objects: `<prefix>/saves/<integration>/<save_id>/`.
pub fn save(prefix: &str, integration: &str, save_id: &str) -> Result<ObjectKey, String> {
    let (p, i, s) = (segment(prefix)?, segment(integration)?, segment(save_id)?);
    ObjectKey::new(&format!("{p}/saves/{i}/{s}/"))
}

/// An agent's transcript: `<prefix>/transcripts/<run>/<branch>/<agent>.jsonl`.
pub fn transcript(prefix: &str, run: &str, branch: &str, agent: &str) -> Result<ObjectKey, String> {
    let (p, r, b, a) = (
        segment(prefix)?,
        segment(run)?,
        segment(branch)?,
        segment(agent)?,
    );
    ObjectKey::new(&format!("{p}/transcripts/{r}/{b}/{a}.jsonl"))
}

/// An export's objects: `<prefix>/exports/<id>/`.
pub fn export(prefix: &str, id: &str) -> Result<ObjectKey, String> {
    let (p, i) = (segment(prefix)?, segment(id)?);
    ObjectKey::new(&format!("{p}/exports/{i}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_refuse_what_climbs() {
        for bad in ["", "/", "a//b", "a/./b", "../a", "/a", "a\\b"] {
            assert!(ObjectKey::new(bad).is_err(), "{bad}");
        }
        assert_eq!(ObjectKey::new("a/b/").unwrap().path().as_ref(), "a/b");
    }
}
