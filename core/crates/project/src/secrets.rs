//! Finds credentials written into a file. Files hold key-store names or environment variable
//! names, never the secret itself.

use serde_json::Value;

/// Prefixes of well-known credential formats.
const PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "AKIA",
    "AIza",
    "-----BEGIN",
];

/// Field names whose literal string value is always a secret.
const SECRET_FIELDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "client_secret",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
];

/// JSON pointers of every value in `value` that looks like a credential, in document order.
pub fn scan(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(value, "", None, &mut found);
    found
}

fn walk(value: &Value, pointer: &str, field: Option<&str>, found: &mut Vec<String>) {
    match value {
        Value::String(s) if is_secret(s, field) => found.push(pointer.to_string()),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, &format!("{pointer}/{i}"), None, found);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                walk(
                    item,
                    &format!("{pointer}/{}", escape(key)),
                    Some(key),
                    found,
                );
            }
        }
        _ => {}
    }
}

fn is_secret(s: &str, field: Option<&str>) -> bool {
    let named = field.is_some_and(|f| SECRET_FIELDS.contains(&f.to_ascii_lowercase().as_str()));
    named || PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Escapes a key for use in a JSON pointer (RFC 6901).
pub fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
