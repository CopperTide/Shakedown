//! The arguments of a tool call, as the tools of REST integrations read them.
use serde_json::Value;

/// The text argument `name` of a tool call, when given and not blank.
pub fn given<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args[name].as_str().filter(|s| !s.trim().is_empty())
}

/// The text argument `name`, or "`name` is required".
pub fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    given(args, name).ok_or_else(|| format!("{name} is required"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_blank_argument_is_not_given() {
        let args = json!({"a": "x", "b": "  ", "c": 3});
        assert_eq!(given(&args, "a"), Some("x"));
        assert_eq!((given(&args, "b"), given(&args, "c")), (None, None));
        assert_eq!(required(&args, "z"), Err("z is required".to_string()));
    }
}
