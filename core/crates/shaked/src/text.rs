//! Small text helpers: cutting long text and JSON to a bound, from the front or the back, always
//! on a char boundary.

use serde_json::Value;

/// `s` cut to at most `max` bytes (on a char boundary), with `…` and the bytes left out when cut.
pub fn cut(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    format!("{}… [{} more bytes]", &s[..i], s.len() - i)
}

/// `v` as compact JSON, cut to `max` bytes; a string is shown as itself, not quoted.
pub fn cut_json(v: &Value, max: usize) -> String {
    match v {
        Value::String(s) => cut(s, max),
        other => cut(&other.to_string(), max),
    }
}

/// `s` with runs of whitespace (newlines included) made one space, then cut to `max` bytes.
pub fn one_line(s: &str, max: usize) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    cut(&joined, max)
}

/// The first `max` characters of `s`, with `… (cut)` when there were more.
pub fn cut_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}… (cut)", &s[..i]),
        None => s.to_string(),
    }
}

/// The last `max` bytes of `s` at most, starting on a char boundary.
pub fn last_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut i = s.len() - max;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    &s[i..]
}

/// The last `n` characters of `s`.
pub fn last_chars(s: &str, n: usize) -> &str {
    let count = s.chars().count();
    match count.checked_sub(n) {
        Some(skip) if skip > 0 => &s[s.char_indices().nth(skip).map_or(0, |(i, _)| i)..],
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cuts_on_char_boundaries_and_says_how_much_was_left_out() {
        assert_eq!(cut("short", 10), "short");
        assert_eq!(cut("abcdef", 3), "abc… [3 more bytes]");
        // "é" is two bytes: a cut inside it moves back.
        assert_eq!(cut("aé", 2), "a… [2 more bytes]");
        assert_eq!(cut_json(&json!("text"), 10), "text");
        assert_eq!(cut_json(&json!({"k": 1}), 100), "{\"k\":1}");
        assert_eq!(one_line("a\n\n  b\tc", 100), "a b c");
    }

    #[test]
    fn cut_chars_counts_characters_and_says_it_cut() {
        assert_eq!(cut_chars("héllo", 2), "hé… (cut)");
        assert_eq!(cut_chars("hé", 2), "hé");
    }

    #[test]
    fn last_bytes_starts_on_a_char_boundary() {
        assert_eq!(last_bytes("abc", 5), "abc");
        assert_eq!(last_bytes("abcdef", 2), "ef");
        // "é" is two bytes: a cut inside it moves on.
        assert_eq!(last_bytes("aé", 1), "");
        assert_eq!(last_bytes("aéb", 3), "éb");
    }

    #[test]
    fn last_chars_cuts_on_a_char_boundary() {
        assert_eq!(last_chars("héllo", 3), "llo");
        assert_eq!(last_chars("hi", 5), "hi");
        assert_eq!(last_chars("ééé", 2), "éé");
    }
}
