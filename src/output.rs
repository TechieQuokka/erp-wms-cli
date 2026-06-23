//! Output rendering: `table` (default, human), `json` (scripting), `csv`.

use std::io::Write;

use clap::ValueEnum;
use serde_json::Value;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Csv,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            "csv" => Ok(OutputFormat::Csv),
            other => Err(format!("unknown output format '{other}' (table|json|csv)")),
        }
    }
}

/// A simple tabular view built by command handlers from typed responses.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new<H: Into<String>>(headers: impl IntoIterator<Item = H>) -> Self {
        Table {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push<C: Into<String>>(&mut self, row: impl IntoIterator<Item = C>) {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }
}

/// Renders a command result. `json` is the raw value printed for `--output json`;
/// `table` is the human/CSV projection. Both describe the same data.
pub fn render(format: OutputFormat, json: &Value, table: &Table) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, json).map_err(anyhow::Error::new)?;
            writeln!(out)?;
        }
        OutputFormat::Csv => write_csv(&mut out, table)?,
        OutputFormat::Table => write_table(&mut out, table)?,
    }
    Ok(())
}

/// Renders a plain message (non-data output, e.g. confirmations) honoring `--output json`.
pub fn render_message(format: OutputFormat, message: &str, json: Value) -> Result<()> {
    match format {
        OutputFormat::Json => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            serde_json::to_writer_pretty(&mut out, &json).map_err(anyhow::Error::new)?;
            writeln!(out)?;
        }
        _ => println!("{message}"),
    }
    Ok(())
}

/// Renders an arbitrary JSON value (used for reports / debug whose shape varies):
/// an array of objects becomes a table; an object becomes a key/value table; a
/// scalar is printed directly. `--output json` always prints the raw value.
pub fn render_auto(format: OutputFormat, json: &Value) -> Result<()> {
    if format == OutputFormat::Json {
        return render(format, json, &Table::new(Vec::<String>::new()));
    }
    let table = auto_table(json);
    render(format, json, &table)
}

fn auto_table(json: &Value) -> Table {
    match json {
        Value::Array(items) => {
            // Column set = keys of the objects, in first-seen order.
            let mut headers: Vec<String> = Vec::new();
            for it in items {
                if let Value::Object(map) = it {
                    for k in map.keys() {
                        if !headers.iter().any(|h| h == k) {
                            headers.push(k.clone());
                        }
                    }
                }
            }
            if headers.is_empty() {
                let mut t = Table::new(["VALUE"]);
                for it in items {
                    t.push([scalar(it)]);
                }
                return t;
            }
            let mut t = Table::new(headers.clone());
            for it in items {
                let row: Vec<String> = headers
                    .iter()
                    .map(|h| scalar(it.get(h).unwrap_or(&Value::Null)))
                    .collect();
                t.push(row);
            }
            t
        }
        Value::Object(map) => {
            let mut t = Table::new(["FIELD", "VALUE"]);
            for (k, v) in map {
                t.push([k.clone(), scalar(v)]);
            }
            t
        }
        other => {
            let mut t = Table::new(["VALUE"]);
            t.push([scalar(other)]);
            t
        }
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn write_table(out: &mut impl Write, table: &Table) -> Result<()> {
    if table.headers.is_empty() {
        return Ok(());
    }
    let mut widths: Vec<usize> = table.headers.iter().map(|h| h.chars().count()).collect();
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }
    let line = |out: &mut dyn Write, cells: &[String]| -> Result<()> {
        let mut parts = Vec::with_capacity(cells.len());
        for (i, cell) in cells.iter().enumerate() {
            let w = widths
                .get(i)
                .copied()
                .unwrap_or_else(|| cell.chars().count());
            parts.push(format!("{cell:<w$}"));
        }
        writeln!(out, "{}", parts.join("  ").trim_end())?;
        Ok(())
    };
    line(out, &table.headers)?;
    // underline
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    line(out, &rule)?;
    if table.rows.is_empty() {
        writeln!(out, "(no rows)")?;
    }
    for row in &table.rows {
        line(out, row)?;
    }
    Ok(())
}

fn write_csv(out: &mut impl Write, table: &Table) -> Result<()> {
    writeln!(out, "{}", csv_row(&table.headers))?;
    for row in &table.rows {
        writeln!(out, "{}", csv_row(row))?;
    }
    Ok(())
}

fn csv_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",")
}

/// RFC-4180 field quoting: quote when the field contains `,` `"` CR or LF.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn csv_field_quoting() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("he said \"hi\""), "\"he said \"\"hi\"\"\"");
        assert_eq!(csv_field("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn output_format_parses() {
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("CSV".parse::<OutputFormat>().unwrap(), OutputFormat::Csv);
        assert!("xml".parse::<OutputFormat>().is_err());
        assert_eq!(OutputFormat::default(), OutputFormat::Table);
    }

    #[test]
    fn auto_table_from_array_unions_keys() {
        let v = json!([{ "a": 1, "b": 2 }, { "a": 3, "c": 4 }]);
        let t = auto_table(&v);
        assert_eq!(t.headers, vec!["a", "b", "c"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], vec!["1", "2", "-"]);
        assert_eq!(t.rows[1], vec!["3", "-", "4"]);
    }

    #[test]
    fn auto_table_from_object_is_kv() {
        let v = json!({ "x": "y" });
        let t = auto_table(&v);
        assert_eq!(t.headers, vec!["FIELD", "VALUE"]);
        assert_eq!(t.rows[0], vec!["x", "y"]);
    }
}
