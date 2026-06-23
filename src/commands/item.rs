//! `wms item` — SKU catalog (read worker+, write admin). Tenant-scoped.

use serde_json::{Value, json};

use crate::cli::ItemAction;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render, render_message};
use crate::util::{field, json_object, s};

pub async fn run(action: ItemAction, ctx: RuntimeContext) -> Result<()> {
    ctx.require_tenant()?; // items are tenant-scoped (X-Tenant carried by the client)
    let client = ctx.client()?;
    match action {
        ItemAction::List { search, status } => {
            let mut q = Vec::new();
            if let Some(v) = search {
                q.push(("search", v));
            }
            if let Some(v) = status {
                q.push(("status", v));
            }
            let rows: Vec<Value> = client.list_all("/items", &q).await?;
            let mut table = Table::new(["SKU", "NAME", "UOM", "MIN", "STATUS"]);
            for r in &rows {
                table.push([
                    field(r, "sku"),
                    field(r, "name"),
                    field(r, "uom"),
                    field(r, "minLevel"),
                    field(r, "status"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        ItemAction::Get { sku } => {
            let it = client.get(&format!("/items/{sku}"), &[]).await?;
            detail(&ctx, &it)
        }
        ItemAction::Create {
            sku,
            name,
            barcode,
            uom,
            min_level,
        } => {
            let body = json_object(vec![
                ("sku", Some(s(sku))),
                ("name", Some(s(name))),
                ("barcode", barcode.map(s)),
                ("uom", uom.map(s)),
                ("minLevel", min_level.map(Value::from)),
            ]);
            let it = client.post("/items", &body).await?;
            detail(&ctx, &it)
        }
        ItemAction::Update {
            sku,
            name,
            barcode,
            uom,
            min_level,
        } => {
            let body = json_object(vec![
                ("name", name.map(s)),
                ("barcode", barcode.map(s)),
                ("uom", uom.map(s)),
                ("minLevel", min_level.map(Value::from)),
            ]);
            let it = client.patch(&format!("/items/{sku}"), &body).await?;
            detail(&ctx, &it)
        }
        ItemAction::Import { file, dry_run } => {
            let res = client.upload_csv("/items:import", &file, dry_run).await?;
            render_message(ctx.output, &import_summary(&res, dry_run), res)
        }
        ItemAction::Disable { sku } => {
            let it = client
                .post_action(&format!("/items/{sku}:disable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Item '{sku}' disabled."), it)
        }
    }
}

fn detail(ctx: &RuntimeContext, it: &Value) -> Result<()> {
    let mut table = Table::new(["FIELD", "VALUE"]);
    for k in [
        "sku",
        "name",
        "barcode",
        "uom",
        "minLevel",
        "status",
        "createdAt",
        "updatedAt",
    ] {
        table.push([k.to_string(), field(it, k)]);
    }
    render(ctx.output, it, &table)
}

/// Human one-liner for an import report `{ created, updated, errors, ... }`.
pub fn import_summary(res: &Value, dry_run: bool) -> String {
    let n = |k: &str| res.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    let prefix = if dry_run { "[dry-run] " } else { "" };
    format!(
        "{prefix}import: {} created, {} updated, {} errors",
        n("created"),
        n("updated"),
        n("errors")
    )
}
