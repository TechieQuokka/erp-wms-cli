//! `wms user` — account administration (admin+; worker/shipper only).

use serde_json::{Value, json};

use crate::cli::UserAction;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render, render_message};
use crate::util::{field, json_object, s};

pub async fn run(action: UserAction, ctx: RuntimeContext) -> Result<()> {
    let client = ctx.client()?;
    match action {
        UserAction::List { tenant, role } => {
            let mut q = Vec::new();
            if let Some(t) = tenant {
                q.push(("tenant", t));
            }
            if let Some(r) = role {
                q.push(("role", r));
            }
            let rows: Vec<Value> = client.list_all("/users", &q).await?;
            let mut table = Table::new(["ID", "EMAIL", "NAME", "ROLE", "STATUS"]);
            for r in &rows {
                table.push([
                    field(r, "id"),
                    field(r, "email"),
                    field(r, "name"),
                    field(r, "role"),
                    field(r, "status"),
                ]);
            }
            render(ctx.output, &Value::Array(rows), &table)
        }
        UserAction::Get { id } => {
            let u = client.get(&format!("/users/{id}"), &[]).await?;
            detail(&ctx, &u)
        }
        UserAction::Create {
            email,
            name,
            role,
            tenant,
            password,
        } => {
            let body = json_object(vec![
                ("email", Some(s(email))),
                ("name", Some(s(name))),
                ("role", Some(s(role))),
                ("tenant", tenant.map(s)),
                ("password", password.map(s)),
            ]);
            let res = client.post("/users", &body).await?;
            created(&ctx, &res)
        }
        UserAction::Update { id, name, email } => {
            let body = json_object(vec![("name", name.map(s)), ("email", email.map(s))]);
            let u = client.patch(&format!("/users/{id}"), &body).await?;
            detail(&ctx, &u)
        }
        UserAction::SetRole { id, role } => {
            let u = client
                .post_action(&format!("/users/{id}:set-role"), &json!({ "role": role }))
                .await?;
            render_message(ctx.output, &format!("Role updated to '{role}'."), u)
        }
        UserAction::Disable { id } => {
            let u = client
                .post_action(&format!("/users/{id}:disable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("User '{id}' disabled."), u)
        }
        UserAction::Enable { id } => {
            let u = client
                .post_action(&format!("/users/{id}:enable"), &json!({}))
                .await?;
            render_message(ctx.output, &format!("User '{id}' enabled."), u)
        }
        UserAction::ResetPassword { id } => {
            let res = client
                .post_action(&format!("/users/{id}:reset-password"), &json!({}))
                .await?;
            let temp = field(&res, "tempPassword");
            render_message(
                ctx.output,
                &format!("Password reset. One-time temporary password: {temp}"),
                res,
            )
        }
    }
}

fn detail(ctx: &RuntimeContext, u: &Value) -> Result<()> {
    let mut table = Table::new(["FIELD", "VALUE"]);
    for k in [
        "id",
        "email",
        "name",
        "role",
        "tenantId",
        "status",
        "mustChangePassword",
        "lastLoginAt",
        "createdAt",
    ] {
        table.push([k.to_string(), field(u, k)]);
    }
    render(ctx.output, u, &table)
}

/// Renders a create response `{ user, tempPassword? }`, surfacing the temp password.
fn created(ctx: &RuntimeContext, res: &Value) -> Result<()> {
    let user = res.get("user").cloned().unwrap_or(Value::Null);
    let temp = res.get("tempPassword").and_then(|t| t.as_str());
    let mut table = Table::new(["FIELD", "VALUE"]);
    for k in ["id", "email", "name", "role", "tenantId", "status"] {
        table.push([k.to_string(), field(&user, k)]);
    }
    if let Some(t) = temp {
        table.push(["tempPassword".to_string(), t.to_string()]);
    }
    render(ctx.output, res, &table)
}
