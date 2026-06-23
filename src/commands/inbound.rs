//! `wms inbound` — ASN receiving & putaway (worker+; shipper reads own).

use serde_json::{Value, json};

use crate::cli::InboundAction;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{Table, render, render_message};
use crate::util::{self, field, json_object, parse_loc_line, parse_qty_line, s};

pub async fn run(action: InboundAction, ctx: RuntimeContext) -> Result<()> {
    ctx.require_tenant()?;
    let client = ctx.client()?;
    match action {
        InboundAction::List { status } => {
            let mut q = Vec::new();
            if let Some(st) = status {
                q.push(("status", st));
            }
            let rows: Vec<Value> = client.list_all("/inbounds", &q).await?;
            let mut table = Table::new(["ID", "REF", "STATUS", "ETA", "CREATED"]);
            for r in &rows {
                table.push([
                    field(r, "id"),
                    field(r, "ref"),
                    field(r, "status"),
                    field(r, "eta"),
                    field(r, "createdAt"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        InboundAction::Get { id } => {
            let v = client.get(&format!("/inbounds/{id}"), &[]).await?;
            render(
                ctx.output,
                &v,
                &kv_table(&v, &["id", "ref", "status", "eta", "createdAt"]),
            )
        }
        InboundAction::Create { r#ref, eta, lines } => {
            let body = json_object(vec![
                ("ref", r#ref.map(s)),
                ("eta", eta.map(s)),
                ("lines", Some(qty_lines(&lines)?)),
            ]);
            let v = client.post("/inbounds", &body).await?;
            render_message(
                ctx.output,
                &format!("Created inbound {}.", field(&v, "id")),
                v,
            )
        }
        InboundAction::Receive {
            id,
            lines,
            putaway_to,
        } => match id {
            Some(id) => {
                // Receive against an existing ASN.
                let body = json!({ "lines": line_array(&lines, false)? });
                let v = client
                    .post_action(&format!("/inbounds/{id}:receive"), &body)
                    .await?;
                render_message(ctx.output, &format!("Received against inbound {id}."), v)
            }
            None => {
                // Blind receive (no prior ASN); optional putaway shortcut.
                let body = json_object(vec![
                    ("lines", Some(line_array(&lines, false)?)),
                    ("putawayTo", putaway_to.map(s)),
                ]);
                let v = client.post("/inbounds:receive", &body).await?;
                render_message(
                    ctx.output,
                    &format!("Blind receive created inbound {}.", field(&v, "id")),
                    v,
                )
            }
        },
        InboundAction::Putaway { id, lines } => {
            let body = json!({ "lines": loc_lines(&lines)? });
            let v = client
                .post_action(&format!("/inbounds/{id}:putaway"), &body)
                .await?;
            render_message(ctx.output, &format!("Put away against inbound {id}."), v)
        }
        InboundAction::Cancel { id } => {
            let v = client
                .post_action(&format!("/inbounds/{id}:cancel"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Inbound {id} cancelled."), v)
        }
    }
}

fn require_lines(lines: &[String]) -> Result<()> {
    if lines.is_empty() {
        return Err(CliError::Usage("at least one --line is required".into()));
    }
    Ok(())
}

/// `[{sku, qty}]` from `<sku>:<qty>` specs (positive quantities for ASN lines).
fn qty_lines(lines: &[String]) -> Result<Value> {
    line_array(lines, false)
}

fn line_array(lines: &[String], _allow_zero: bool) -> Result<Value> {
    require_lines(lines)?;
    let mut arr = Vec::new();
    for l in lines {
        let (sku, qty) = parse_qty_line(l)?;
        arr.push(json!({ "sku": sku, "qty": qty }));
    }
    Ok(Value::Array(arr))
}

/// `[{sku, location, qty}]` from `<sku>@<location>:<qty>` specs.
fn loc_lines(lines: &[String]) -> Result<Value> {
    require_lines(lines)?;
    let mut arr = Vec::new();
    for l in lines {
        let (sku, location, qty) = parse_loc_line(l)?;
        arr.push(json!({ "sku": sku, "location": location, "qty": qty }));
    }
    Ok(Value::Array(arr))
}

fn kv_table(v: &Value, keys: &[&str]) -> Table {
    let mut t = Table::new(["FIELD", "VALUE"]);
    for k in keys {
        t.push([k.to_string(), util::field(v, k)]);
    }
    t
}
