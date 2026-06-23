//! `wms dev` — developer/system layer (developer only).

use serde_json::{Value, json};

use crate::cli::{DevAction, DevKeyAction, DevTenantAction, DevUserAction};
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render, render_auto, render_message};
use crate::util::{self, field, json_object, s};

pub async fn run(action: DevAction, ctx: RuntimeContext) -> Result<()> {
    let client = ctx.client()?;
    match action {
        DevAction::Key { action } => key(&ctx, &client, action).await,
        DevAction::Query { sql } => {
            let res = client
                .post_action("/dev/query", &json!({ "sql": sql }))
                .await?;
            let rows = res.get("rows").cloned().unwrap_or(Value::Array(vec![]));
            if let Some(true) = res.get("truncated").and_then(|t| t.as_bool()) {
                eprintln!("note: result truncated to the row cap");
            }
            render_auto(ctx.output, &rows)
        }
        DevAction::User { action } => {
            let DevUserAction::Grant { id, role } = action;
            util::confirm(
                &ctx,
                &format!("Grant '{role}' to user {id} — privilege escalation."),
            )?;
            let v = client
                .post_action(&format!("/dev/users/{id}:grant"), &json!({ "role": role }))
                .await?;
            render_message(ctx.output, &format!("Granted '{role}' to {id}."), v)
        }
        DevAction::Tenant { action } => {
            let DevTenantAction::Delete { code } = action;
            util::confirm_phrase(
                &ctx,
                &format!("Permanently DELETE tenant '{code}' and all its data."),
                &code,
            )?;
            let v = client.delete(&format!("/dev/tenants/{code}"), &[]).await?;
            render_message(
                ctx.output,
                &format!("Tenant '{code}' permanently deleted."),
                v,
            )
        }
        DevAction::Audit { action } => {
            let mut q = Vec::new();
            if let Some(a) = action {
                q.push(("action", a));
            }
            let v = client.get("/dev/audit", &q).await?;
            // Audit may return an array or an envelope; render whichever.
            let data = v.get("data").cloned().unwrap_or(v);
            render_auto(ctx.output, &data)
        }
        DevAction::Debug => {
            let v = client.get("/dev/debug", &[]).await?;
            render_auto(ctx.output, &v)
        }
    }
}

async fn key(
    ctx: &RuntimeContext,
    client: &crate::client::ApiClient,
    action: DevKeyAction,
) -> Result<()> {
    match action {
        DevKeyAction::List => {
            let rows: Vec<Value> = client.list_all("/dev/keys", &[]).await?;
            let mut table = Table::new([
                "ID", "NAME", "PREFIX", "ROLE", "TENANT", "EXPIRES", "REVOKED",
            ]);
            for r in &rows {
                table.push([
                    field(r, "id"),
                    field(r, "name"),
                    field(r, "prefix"),
                    field(r, "role"),
                    field(r, "tenantId"),
                    field(r, "expiresAt"),
                    field(r, "revokedAt"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        DevKeyAction::Create {
            name,
            role,
            tenant,
            expires,
        } => {
            let body = json_object(vec![
                ("name", Some(s(name))),
                ("role", Some(s(role))),
                ("tenant", tenant.map(s)),
                ("expiresAt", expires.map(s)),
            ]);
            let v = client.post("/dev/keys", &body).await?;
            // The full key is shown once — surface it clearly.
            let key = field(&v, "key");
            render_message(
                ctx.output,
                &format!(
                    "API key created (store it now — shown only once):\n  {key}\n  id: {}",
                    field(&v, "id")
                ),
                v,
            )
        }
        DevKeyAction::Revoke { id } => {
            let v = client
                .post_action(&format!("/dev/keys/{id}:revoke"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("API key {id} revoked."), v)
        }
    }
}
