//! Secrets a team carries (an `http` agent's request headers) that must never reach a log, an
//! answer or a saved run. A header value is put in this process's memory the moment it is read
//! and stands everywhere else as a reference, written `{"secret": "<ref>"}`. The reference is good
//! until shaked stops: a team stored before then names a secret that is gone, and says so.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock},
};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// A kept value and the scope (a caller's scope) that kept it.
type Kept = (String, String);

fn vault() -> &'static Mutex<HashMap<String, Kept>> {
    static VAULT: OnceLock<Mutex<HashMap<String, Kept>>> = OnceLock::new();
    VAULT.get_or_init(Default::default)
}

tokio::task_local! {
    /// The scope of the request being served: what a header value read now is kept for.
    static SCOPE: String;
}

/// Runs `work` with `scope` as the scope of every header value read in it (a request's caller).
pub async fn scoped<F: std::future::Future>(scope: String, work: F) -> F::Output {
    SCOPE.scope(scope, work).await
}

/// The scope of the request being served; empty outside a request.
fn current_scope() -> String {
    SCOPE.try_with(Clone::clone).unwrap_or_default()
}

/// Keeps `value` in memory for the current scope and gives back its reference.
fn keep(value: String) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    vault()
        .lock()
        .unwrap()
        .insert(id.clone(), (current_scope(), value));
    id
}

/// The value `reference` stands for, while this process holds it and only for the scope that
/// kept it: another caller's reference reads as gone.
fn reveal(reference: &str, scope: &str) -> Option<String> {
    let kept = vault().lock().unwrap();
    kept.get(reference)
        .filter(|(owner, _)| owner == scope)
        .map(|(_, value)| value.clone())
}

/// An `http` agent's request headers: each name with a reference to its value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers(BTreeMap<String, String>);

/// Why a header's value can't be had any more.
pub fn gone(name: &str) -> String {
    format!(
        "the value of header {name:?} was kept in memory by a shaked that has stopped since: connect the agent again"
    )
}

impl Headers {
    /// Headers with these plain values (kept in memory from here on).
    pub fn from_plain(plain: impl IntoIterator<Item = (String, String)>) -> Headers {
        Headers(plain.into_iter().map(|(k, v)| (k, keep(v))).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Each name with its value, for the current request's scope; `Err` names the first header
    /// whose value is gone or was kept for someone else.
    pub fn reveal(&self) -> Result<Vec<(String, String)>, String> {
        self.reveal_for(&current_scope())
    }

    /// The same for `scope` (a run reads its headers for the caller it runs for).
    pub fn reveal_for(&self, scope: &str) -> Result<Vec<(String, String)>, String> {
        let one =
            |(k, r): (&String, &String)| reveal(r, scope).map(|v| (k.clone(), v)).ok_or(gone(k));
        self.0.iter().map(one).collect()
    }
}

/// A header value as written: `{"secret": ref}`.
#[derive(Serialize, Deserialize)]
struct Ref {
    secret: String,
}

/// A header value as read: a plain value (kept from here on) or a reference.
#[derive(Deserialize)]
#[serde(untagged)]
enum ValueIn {
    Plain(String),
    Ref(Ref),
}

impl Serialize for Headers {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let refs = self.0.iter().map(|(k, r)| (k, Ref { secret: r.clone() }));
        s.collect_map(refs)
    }
}

impl<'de> Deserialize<'de> for Headers {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Headers, D::Error> {
        let raw = BTreeMap::<String, ValueIn>::deserialize(d)?;
        let mut out = BTreeMap::new();
        for (name, v) in raw {
            if name.trim().is_empty() {
                return Err(de::Error::custom("a header needs a name"));
            }
            let reference = match v {
                ValueIn::Plain(value) => keep(value),
                ValueIn::Ref(r) => r.secret,
            };
            out.insert(name, reference);
        }
        Ok(Headers(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_value_never_comes_back_out_but_its_reference_does() {
        let h: Headers = serde_json::from_value(json!({"Authorization": "Bearer s3cret"})).unwrap();
        let written = serde_json::to_string(&h).unwrap();
        assert!(!written.contains("s3cret"), "{written}");
        assert!(!format!("{h:?}").contains("s3cret"));
        // What was written reads back as the same secret.
        let again: Headers = serde_json::from_str(&written).unwrap();
        assert_eq!(again, h);
        assert_eq!(
            again.reveal().unwrap(),
            [("Authorization".to_string(), "Bearer s3cret".to_string())]
        );
    }

    #[tokio::test]
    async fn a_reference_is_good_only_for_the_scope_that_kept_it() {
        let h: Headers = scoped("scope-a".into(), async {
            serde_json::from_value(json!({"X-Key": "mine"})).unwrap()
        })
        .await;
        let written = serde_json::to_string(&h).unwrap();
        assert_eq!(h.reveal_for("scope-a").unwrap()[0].1, "mine");
        // Another scope citing the reference gets nothing, and the same message as for a gone one.
        let cited: Headers = scoped("scope-b".into(), async {
            serde_json::from_str(&written).unwrap()
        })
        .await;
        let e = cited.reveal_for("scope-b").unwrap_err();
        assert!(e.contains("connect the agent again"), "{e}");
    }

    #[test]
    fn a_reference_this_process_never_kept_is_gone() {
        let h: Headers = serde_json::from_value(json!({"X-Key": {"secret": "nope"}})).unwrap();
        let e = h.reveal().unwrap_err();
        assert!(
            e.contains("X-Key") && e.contains("connect the agent again"),
            "{e}"
        );
        assert!(serde_json::from_value::<Headers>(json!({" ": "v"})).is_err());
    }
}
