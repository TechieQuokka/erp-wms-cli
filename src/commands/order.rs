//! `wms order` — outbound orders (worker+; shipper reads own).

use serde_json::{Value, json};

use crate::cli::OrderAction;
use crate::commands::item::import_summary;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{Table, render, render_message};
use crate::util::{self, field, json_object, parse_loc_line, parse_qty_line, s};

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
            let res = client.upload_csv("/orders:import", &file, dry_run).await?;
            render_message(ctx.output, &import_summary(&res, dry_run), res)
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
