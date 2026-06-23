//! `wms alert` — dashboard alerts (tenant-scoped, worker+).

use serde_json::Value;

use crate::cli::AlertAction;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{OutputFormat, Table, render, render_auto};
use crate::util::field;

pub async fn run(action: AlertAction, ctx: RuntimeContext) -> Result<()> {
    let AlertAction::List = action;
    ctx.require_tenant()?;
    let client = ctx.client()?;
    let v = client.get("/alerts", &[]).await?;

    if ctx.output == OutputFormat::Json {
        return render_auto(ctx.output, &v);
    }

    // Low stock section.
    let low = v
        .get("lowStock")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let mut low_table = Table::new(["SKU", "NAME", "ON_HAND", "MIN_LEVEL"]);
    for r in &low {
        low_table.push([
            field(r, "sku"),
            field(r, "name"),
            field(r, "onHand"),
            field(r, "minLevel"),
        ]);
    }
    println!("Low stock ({}):", low.len());
    render(ctx.output, &Value::Array(low.clone()), &low_table)?;

    // Unprocessed orders section.
    let unproc = v
        .get("unprocessedOrders")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let mut ord_table = Table::new(["ID", "REF", "STATUS", "CREATED"]);
    for r in &unproc {
        ord_table.push([
            field(r, "id"),
            field(r, "ref"),
            field(r, "status"),
            field(r, "createdAt"),
        ]);
    }
    println!("\nUnprocessed orders ({}):", unproc.len());
    render(ctx.output, &Value::Array(unproc), &ord_table)
}
