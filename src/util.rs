//! Small shared helpers: line-spec parsing, confirmations, JSON field access.

use std::io::Write;

use serde_json::Value;

use crate::context::RuntimeContext;
use crate::error::{CliError, Result};

/// Parses a `<sku>:<qty>` line spec.
pub fn parse_qty_line(s: &str) -> Result<(String, i64)> {
    let (sku, qty) = s
        .rsplit_once(':')
        .ok_or_else(|| CliError::Usage(format!("line '{s}' must look like <sku>:<qty>")))?;
    let qty: i64 = qty
        .trim()
        .parse()
        .map_err(|_| CliError::Usage(format!("invalid quantity in '{s}'")))?;
    if sku.trim().is_empty() {
        return Err(CliError::Usage(format!("missing sku in '{s}'")));
    }
    Ok((sku.trim().to_string(), qty))
}

/// Parses a `<sku>@<location>:<qty>` line spec.
pub fn parse_loc_line(s: &str) -> Result<(String, String, i64)> {
    let (head, qty) = s.rsplit_once(':').ok_or_else(|| {
        CliError::Usage(format!("line '{s}' must look like <sku>@<location>:<qty>"))
    })?;
    let (sku, location) = head.split_once('@').ok_or_else(|| {
        CliError::Usage(format!("line '{s}' must look like <sku>@<location>:<qty>"))
    })?;
    let qty: i64 = qty
        .trim()
        .parse()
        .map_err(|_| CliError::Usage(format!("invalid quantity in '{s}'")))?;
    if sku.trim().is_empty() || location.trim().is_empty() {
        return Err(CliError::Usage(format!("missing sku or location in '{s}'")));
    }
    Ok((sku.trim().to_string(), location.trim().to_string(), qty))
}

/// Confirms a guarded action. Returns Ok when `--yes` is set or the user types `y`.
pub fn confirm(ctx: &RuntimeContext, prompt: &str) -> Result<()> {
    if ctx.assume_yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Usage(format!(
            "{prompt} — refusing in a non-interactive run; pass --yes to proceed"
        )));
    }
    eprint!("{prompt} [y/N]: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    if line.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        Err(CliError::Usage("aborted".into()))
    }
}

/// Confirms a destructive (⚠⚠) action by requiring the exact phrase to be typed.
pub fn confirm_phrase(ctx: &RuntimeContext, prompt: &str, phrase: &str) -> Result<()> {
    if !ctx.assume_yes {
        return Err(CliError::Usage(format!(
            "{prompt} — this is destructive; pass --yes and confirm the phrase"
        )));
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Usage(format!(
            "{prompt} — refusing in a non-interactive run without an explicit confirmation phrase"
        )));
    }
    eprint!("{prompt}\n  type '{phrase}' to confirm: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    if line.trim() == phrase {
        Ok(())
    } else {
        Err(CliError::Usage(
            "confirmation phrase did not match — aborted".into(),
        ))
    }
}

use std::io::IsTerminal;

/// A JSON field as a display string (`-` for null/absent).
pub fn field(v: &Value, key: &str) -> String {
    match v.get(key) {
        None | Some(Value::Null) => "-".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Builds a JSON object from `(key, Option<value>)` pairs, skipping `None`.
pub fn json_object(pairs: Vec<(&str, Option<Value>)>) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        if let Some(val) = v {
            map.insert(k.to_string(), val);
        }
    }
    Value::Object(map)
}

/// Convenience: wrap a string in a JSON value.
pub fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qty_line_parses() {
        assert_eq!(parse_qty_line("ABC:5").unwrap(), ("ABC".into(), 5));
        assert_eq!(
            parse_qty_line(" SKU-1 : 12 ").unwrap(),
            ("SKU-1".into(), 12)
        );
    }

    #[test]
    fn qty_line_rejects_bad_input() {
        assert!(parse_qty_line("ABC").is_err());
        assert!(parse_qty_line("ABC:not-a-number").is_err());
        assert!(parse_qty_line(":5").is_err());
    }

    #[test]
    fn loc_line_parses() {
        assert_eq!(
            parse_loc_line("SKU@A-01:7").unwrap(),
            ("SKU".into(), "A-01".into(), 7)
        );
    }

    #[test]
    fn loc_line_rejects_bad_input() {
        assert!(parse_loc_line("SKU:7").is_err()); // missing @location
        assert!(parse_loc_line("SKU@A-01").is_err()); // missing :qty
        assert!(parse_loc_line("@A-01:7").is_err()); // missing sku
    }

    #[test]
    fn json_object_skips_none() {
        let v = json_object(vec![("a", Some(s("x"))), ("b", None)]);
        assert_eq!(v.get("a").and_then(|x| x.as_str()), Some("x"));
        assert!(v.get("b").is_none());
    }

    #[test]
    fn field_renders_scalars_and_missing() {
        let v = serde_json::json!({ "name": "Acme", "n": 3, "nil": null });
        assert_eq!(field(&v, "name"), "Acme");
        assert_eq!(field(&v, "n"), "3");
        assert_eq!(field(&v, "nil"), "-");
        assert_eq!(field(&v, "absent"), "-");
    }
}
