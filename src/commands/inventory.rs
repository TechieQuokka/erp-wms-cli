//! `wms inventory` — stock (worker+; adjust is admin & audited).

use serde_json::{Value, json};

use crate::cli::InventoryAction;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{Table, render, render_message};
use crate::util::{self, field, json_object, parse_qty_line, s};

pub async fn run(action: InventoryAction, ctx: RuntimeContext) -> Result<()> {
    ctx.require_tenant()?;
    let client = ctx.client()?;
    match action {
        InventoryAction::List {
            sku,
            location,
            zone,
            below,
        } => {
            let mut q = Vec::new();
            if let Some(v) = sku {
                q.push(("sku", v));
            }
            if let Some(v) = location {
                q.push(("location", v));
            }
            if let Some(v) = zone {
                q.push(("zone", v));
            }
            if let Some(v) = below {
                q.push(("below", v.to_string()));
            }
            let rows: Vec<Value> = client.list_all("/inventory", &q).await?;
            let mut table = Table::new(["SKU", "LOCATION", "ON_HAND", "RESERVED", "AVAILABLE"]);
            for r in &rows {
                table.push([
                    field(r, "sku"),
                    field(r, "location"),
                    field(r, "qtyOnHand"),
                    field(r, "qtyReserved"),
                    field(r, "qtyAvailable"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        InventoryAction::Get { sku } => {
            let v = client.get(&format!("/inventory/{sku}"), &[]).await?;
            // Totals + per-location breakdown; show locations when present.
            let mut table = Table::new(["LOCATION", "ON_HAND", "RESERVED", "AVAILABLE"]);
            if let Some(locs) = v.get("locations").and_then(|l| l.as_array()) {
                for r in locs {
                    table.push([
                        field(r, "location"),
                        field(r, "qtyOnHand"),
                        field(r, "qtyReserved"),
                        field(r, "qtyAvailable"),
                    ]);
                }
            }
            render(ctx.output, &v, &table)
        }
        InventoryAction::Move { sku, from, to, qty } => {
            let body = json!({ "sku": sku, "from": from, "to": to, "qty": qty });
            let v = client.post_action("/inventory:move", &body).await?;
            render_message(ctx.output, &format!("Moved {qty} {sku}: {from} → {to}."), v)
        }
        InventoryAction::Count { location, lines } => {
            if lines.is_empty() {
                return Err(CliError::Usage(
                    "at least one --line <sku>:<counted> is required".into(),
                ));
            }
            let mut arr = Vec::new();
            for l in &lines {
                let (sku, counted) = parse_qty_line(l)?;
                arr.push(json!({ "sku": sku, "counted": counted }));
            }
            let body = json!({ "location": location, "lines": Value::Array(arr) });
            let v = client.post_action("/inventory:count", &body).await?;
            render_message(ctx.output, &format!("Recorded count at {location}."), v)
        }
        InventoryAction::Ledger {
            sku,
            location,
            since,
            until,
        } => {
            let mut q = Vec::new();
            for (k, val) in [
                ("sku", sku),
                ("location", location),
                ("since", since),
                ("until", until),
            ] {
                if let Some(v) = val {
                    q.push((k, v));
                }
            }
            let rows: Vec<Value> = client.list_all("/inventory/ledger", &q).await?;
            let mut table = Table::new(["TIME", "SKU", "LOCATION", "DELTA", "TYPE", "REF"]);
            for r in &rows {
                table.push([
                    field(r, "createdAt"),
                    field(r, "sku"),
                    field(r, "location"),
                    field(r, "delta"),
                    field(r, "type"),
                    field(r, "refType"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        InventoryAction::Adjust {
            sku,
            location,
            qty,
            set,
            reason,
        } => {
            if qty.is_none() && set.is_none() {
                return Err(CliError::Usage(
                    "provide either --qty <delta> or --set <abs>".into(),
                ));
            }
            let what = match (qty, set) {
                (Some(d), _) => format!("delta {d:+}"),
                (_, Some(a)) => format!("set {a}"),
                _ => unreachable!(),
            };
            util::confirm(
                &ctx,
                &format!("Adjust {sku} @ {location} ({what}) — this is audited."),
            )?;
            let body = json_object(vec![
                ("sku", Some(s(sku.clone()))),
                ("location", Some(s(location.clone()))),
                ("qty", qty.map(Value::from)),
                ("set", set.map(Value::from)),
                ("reason", Some(s(reason))),
            ]);
            let v = client.post_action("/inventory:adjust", &body).await?;
            render_message(
                ctx.output,
                &format!("Adjusted {sku} @ {location} ({what})."),
                v,
            )
        }
    }
}
