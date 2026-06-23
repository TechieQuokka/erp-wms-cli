//! `wms tenant` — shipper/tenant administration (admin+).

use serde_json::{Value, json};

use crate::cli::TenantAction;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render, render_message};
use crate::util::{self, field, json_object, s};

pub async fn run(action: TenantAction, ctx: RuntimeContext) -> Result<()> {
    let client = ctx.client()?;
    match action {
        TenantAction::List { status } => {
            let mut q = Vec::new();
            if let Some(st) = status {
                q.push(("status", st));
            }
            let rows: Vec<Value> = client.list_all("/tenants", &q).await?;
            let mut table = Table::new(["CODE", "NAME", "STATUS", "CURRENCY"]);
            for r in &rows {
                table.push([
                    field(r, "code"),
                    field(r, "name"),
                    field(r, "status"),
                    field(r, "defaultCurrency"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        TenantAction::Get { code } => {
            let t = client.get(&format!("/tenants/{code}"), &[]).await?;
            detail(&ctx, &t)
        }
        TenantAction::Create {
            name,
            code,
            contact_email,
            currency,
        } => {
            let body = json_object(vec![
                ("code", Some(s(code))),
                ("name", Some(s(name))),
                ("contactEmail", contact_email.map(s)),
                ("defaultCurrency", currency.map(s)),
            ]);
            let t = client.post("/tenants", &body).await?;
            detail(&ctx, &t)
        }
        TenantAction::Update {
            code,
            name,
            contact_email,
            currency,
        } => {
            let body = json_object(vec![
                ("name", name.map(s)),
                ("contactEmail", contact_email.map(s)),
                ("defaultCurrency", currency.map(s)),
            ]);
            let t = client.patch(&format!("/tenants/{code}"), &body).await?;
            detail(&ctx, &t)
        }
        TenantAction::Disable { code } => {
            let t = client
                .post_action(&format!("/tenants/{code}:disable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Tenant '{code}' disabled."), t)
        }
        TenantAction::Enable { code } => {
            let t = client
                .post_action(&format!("/tenants/{code}:enable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("Tenant '{code}' enabled."), t)
        }
    }
}

fn detail(ctx: &RuntimeContext, t: &Value) -> Result<()> {
    let mut table = Table::new(["FIELD", "VALUE"]);
    for k in [
        "code",
        "name",
        "status",
        "defaultCurrency",
        "contactName",
        "contactEmail",
        "contactPhone",
        "createdAt",
        "updatedAt",
    ] {
        table.push([k.to_string(), util::field(t, k)]);
    }
    render(ctx.output, t, &table)
}
