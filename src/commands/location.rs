//! `wms location` — warehouse-global locations (read worker+, write admin).

use serde_json::{Value, json};

use crate::cli::LocationAction;
use crate::commands::item::import_summary;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render, render_message};
use crate::util::{field, json_object, s};

pub async fn run(action: LocationAction, ctx: RuntimeContext) -> Result<()> {
    let client = ctx.client()?;
    match action {
        LocationAction::List {
            zone,
            r#type,
            empty,
        } => {
            let mut q = Vec::new();
            if let Some(v) = zone {
                q.push(("zone", v));
            }
            if let Some(v) = r#type {
                q.push(("type", v));
            }
            if empty {
                q.push(("empty", "true".to_string()));
            }
            let rows: Vec<Value> = client.list_all("/locations", &q).await?;
            let mut table = Table::new(["CODE", "ZONE", "TYPE", "CAPACITY", "STATUS"]);
            for r in &rows {
                table.push([
                    field(r, "code"),
                    field(r, "zone"),
                    field(r, "type"),
                    field(r, "capacity"),
                    field(r, "status"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        LocationAction::Get { code } => {
            let l = client.get(&format!("/locations/{code}"), &[]).await?;
            detail(&ctx, &l)
        }
        LocationAction::Create {
            code,
            zone,
            r#type,
            capacity,
        } => {
            let body = json_object(vec![
                ("code", Some(s(code))),
                ("zone", zone.map(s)),
                ("type", r#type.map(s)),
                ("capacity", capacity.map(Value::from)),
            ]);
            let l = client.post("/locations", &body).await?;
            detail(&ctx, &l)
        }
        LocationAction::Update {
            code,
            zone,
            r#type,
            capacity,
        } => {
            let body = json_object(vec![
                ("zone", zone.map(s)),
                ("type", r#type.map(s)),
                ("capacity", capacity.map(Value::from)),
            ]);
            let l = client.patch(&format!("/locations/{code}"), &body).await?;
            detail(&ctx, &l)
        }
        LocationAction::Import { file, dry_run } => {
            let res = client
                .upload_csv("/locations:import", &file, dry_run)
                .await?;
            render_message(ctx.output, &import_summary(&res, dry_run), res)
        }
        LocationAction::Disable { code } => {
            let l = client
                .post_action(&format!("/locations/{code}:disable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Location '{code}' disabled."), l)
        }
    }
}

fn detail(ctx: &RuntimeContext, l: &Value) -> Result<()> {
    let mut table = Table::new(["FIELD", "VALUE"]);
    for k in [
        "code",
        "zone",
        "type",
        "capacity",
        "status",
        "createdAt",
        "updatedAt",
    ] {
        table.push([k.to_string(), field(l, k)]);
    }
    render(ctx.output, l, &table)
}
