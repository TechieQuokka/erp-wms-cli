//! `wms order` — outbound orders (worker+; shipper reads own).

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Value, json};

use crate::cli::OrderAction;
use crate::client::ApiClient;
use crate::commands::item::import_summary;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{OutputFormat, Table, render, render_message};
use crate::util::{self, field, json_object, parse_loc_line, parse_qty_line, s};

/// Max distinct orders per import request (matches the backend cap; bounded by
/// the Workers per-request subrequest budget).
const ORDER_IMPORT_CHUNK: usize = 50;

/// Imports an orders CSV, splitting large files into batches of at most
/// `ORDER_IMPORT_CHUNK` orders so each request stays within the backend limit.
/// When applying multiple batches it dry-runs them all first (validate-all → no
/// partial writes if any row is invalid), then applies, and aggregates the report.
async fn import_orders_chunked(
    client: &ApiClient,
    file: &Path,
    dry_run: bool,
    output: OutputFormat,
) -> Result<()> {
    let text = std::fs::read_to_string(file)?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| CliError::Usage("CSV is empty".into()))?;

    // Group data rows by order_ref (first column), preserving first-seen order.
    let mut seen: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for line in lines {
        let key = line
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .to_string();
        if !groups.contains_key(&key) {
            seen.push(key.clone());
        }
        groups.entry(key).or_default().push(line.to_string());
    }
    if seen.is_empty() {
        return Err(CliError::Usage("CSV has no data rows".into()));
    }

    // Build one CSV body per batch of <= ORDER_IMPORT_CHUNK distinct refs.
    let batches: Vec<Vec<u8>> = seen
        .chunks(ORDER_IMPORT_CHUNK)
        .map(|refs| {
            let mut body = String::from(header);
            body.push('\n');
            for r in refs {
                for l in &groups[r] {
                    body.push_str(l);
                    body.push('\n');
                }
            }
            body.into_bytes()
        })
        .collect();

    let multi = batches.len() > 1;
    // Validate everything before writing anything (preserve atomic-on-validation).
    if !dry_run && multi {
        for b in &batches {
            client
                .upload_csv_bytes("/orders:import", "orders.csv", b.clone(), true)
                .await?;
        }
    }

    let (mut created, mut updated) = (0i64, 0i64);
    let mut last = Value::Null;
    for b in &batches {
        let rep = client
            .upload_csv_bytes("/orders:import", "orders.csv", b.clone(), dry_run)
            .await?;
        created += rep.get("created").and_then(Value::as_i64).unwrap_or(0);
        updated += rep.get("updated").and_then(Value::as_i64).unwrap_or(0);
        last = rep;
    }

    if multi {
        let agg = json!({ "created": created, "updated": updated, "errors": [], "batches": batches.len() });
        let msg = format!(
            "{} ({} batches)",
            import_summary(&agg, dry_run),
            batches.len()
        );
        render_message(output, &msg, agg)
    } else {
        render_message(output, &import_summary(&last, dry_run), last)
    }
}

pub async fn run(action: OrderAction, ctx: RuntimeContext) -> Result<()> {
    ctx.require_tenant()?;
    let client = ctx.client()?;
    match action {
        OrderAction::List { status } => {
            let mut q = Vec::new();
            if let Some(st) = status {
                q.push(("status", st));
            }
            let rows: Vec<Value> = client.list_all("/orders", &q).await?;
            let mut table = Table::new(["ID", "REF", "STATUS", "LINES", "CREATED"]);
            for r in &rows {
                let lines = r.get("lines").and_then(|l| l.as_array()).map(|a| a.len());
                table.push([
                    field(r, "id"),
                    field(r, "ref"),
                    field(r, "status"),
                    lines.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
                    field(r, "createdAt"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        OrderAction::Get { id } => {
            let v = client.get(&format!("/orders/{id}"), &[]).await?;
            let mut table = Table::new(["FIELD", "VALUE"]);
            for k in ["id", "ref", "status", "createdAt"] {
                table.push([k.to_string(), util::field(&v, k)]);
            }
            render(ctx.output, &v, &table)
        }
        OrderAction::Create {
            r#ref,
            ship_to,
            lines,
        } => {
            let ship = ship_to.map(|name| json!({ "name": name }));
            let body = json_object(vec![
                ("ref", r#ref.map(s)),
                ("shipTo", ship),
                ("lines", Some(qty_lines(&lines)?)),
            ]);
            let v = client.post("/orders", &body).await?;
            render_message(
                ctx.output,
                &format!(
                    "Created order {} (status: {}).",
                    field(&v, "id"),
                    field(&v, "status")
                ),
                v,
            )
        }
        OrderAction::Import { file, dry_run } => {
            import_orders_chunked(&client, &file, dry_run, ctx.output).await
        }
        OrderAction::Allocate { id } => {
            let v = client
                .post_action(&format!("/orders/{id}:allocate"), &json!({}))
                .await?;
            render_message(
                ctx.output,
                &format!("Allocated order {id} (status: {}).", field(&v, "status")),
                v,
            )
        }
        OrderAction::Pick { id, lines } => {
            let body = json!({ "lines": pick_lines(&lines)? });
            let v = client
                .post_action(&format!("/orders/{id}:pick"), &body)
                .await?;
            render_message(ctx.output, &format!("Picked order {id}."), v)
        }
        OrderAction::Pack { id } => {
            let v = client
                .post_action(&format!("/orders/{id}:pack"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Packed order {id}."), v)
        }
        OrderAction::Ship { id, tracking } => {
            let body = json_object(vec![("tracking", tracking.map(s))]);
            let v = client
                .post_action(&format!("/orders/{id}:ship"), &body)
                .await?;
            render_message(ctx.output, &format!("Shipped order {id}."), v)
        }
        OrderAction::Cancel { id, reason } => {
            let v = client
                .post_action(
                    &format!("/orders/{id}:cancel"),
                    &json!({ "reason": reason }),
                )
                .await?;
            render_message(ctx.output, &format!("Cancelled order {id}."), v)
        }
    }
}

fn qty_lines(lines: &[String]) -> Result<Value> {
    if lines.is_empty() {
        return Err(CliError::Usage(
            "at least one --line <sku>:<qty> is required".into(),
        ));
    }
    let mut arr = Vec::new();
    for l in lines {
        let (sku, qty) = parse_qty_line(l)?;
        arr.push(json!({ "sku": sku, "qty": qty }));
    }
    Ok(Value::Array(arr))
}

/// Pick lines: the API picks by `{sku, qty}`. A `<sku>@<location>:<qty>` form is
/// accepted for symmetry with putaway; the location is informational only.
fn pick_lines(lines: &[String]) -> Result<Value> {
    if lines.is_empty() {
        return Err(CliError::Usage("at least one --line is required".into()));
    }
    let mut arr = Vec::new();
    for l in lines {
        let (sku, qty) = if l.contains('@') {
            let (sku, _loc, qty) = parse_loc_line(l)?;
            (sku, qty)
        } else {
            parse_qty_line(l)?
        };
        arr.push(json!({ "sku": sku, "qty": qty }));
    }
    Ok(Value::Array(arr))
}
