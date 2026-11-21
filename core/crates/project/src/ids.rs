//! Item ids. An id becomes a file or folder name, so it is checked before any filesystem call.

use std::fmt;

use crate::error::ProjectError;

/// The longest id allowed.
const MAX_LEN: usize = 64;

/// A checked item id: 1–64 lowercase ASCII letters, digits and dashes, not starting with a dash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(String);

impl Id {
    /// Checks `s` and wraps it.
    pub fn parse(s: &str) -> Result<Id, ProjectError> {
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        let ok = (1..=MAX_LEN).contains(&s.len()) && !s.starts_with('-') && s.chars().all(allowed);
        if ok {
            Ok(Id(s.to_string()))
        } else {
            Err(ProjectError::InvalidId(s.to_string()))
        }
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
