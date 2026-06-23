//! `wms auth` — login / logout / whoami / token.

use serde_json::json;

use crate::cli::AuthAction;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{Table, render, render_message};
use crate::util;

pub async fn run(action: AuthAction, mut ctx: RuntimeContext) -> Result<()> {
    match action {
        AuthAction::Login { email, password } => login(&mut ctx, email, password).await,
        AuthAction::Logout => logout(&mut ctx).await,
        AuthAction::Whoami => whoami(&ctx).await,
        AuthAction::Token => token(&ctx),
    }
}

async fn login(
    ctx: &mut RuntimeContext,
    email: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let email = match email {
        Some(e) => e,
        None => prompt_line("Email: ")?,
    };
    let password = match password {
        Some(p) => p,
        None => rpassword::prompt_password("Password: ").map_err(|e| CliError::Other(e.into()))?,
    };

    let client = ctx.client_anon()?;
    let resp = client
        .post_action(
            "/auth/login",
            &json!({ "email": email, "password": password }),
        )
        .await?;
    let token = resp
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| CliError::Other(anyhow::anyhow!("login response had no token")))?;

    // Persist the session token to the active profile's credentials (mode 0600).
    let profile = ctx.profile.clone();
    ctx.store.creds_mut(&profile).token = Some(token.to_string());
    ctx.store.save_credentials()?;

    let role = resp
        .get("user")
        .map(|u| util::field(u, "role"))
        .unwrap_or_else(|| "-".into());
    render_message(
        ctx.output,
        &format!("Logged in to profile '{profile}' (role: {role})."),
        json!({ "ok": true, "profile": profile, "user": resp.get("user") }),
    )
}

async fn logout(ctx: &mut RuntimeContext) -> Result<()> {
    // Best-effort server-side revocation; clear the local token regardless.
    if let Ok(client) = ctx.client() {
        let _ = client.post_action("/auth/logout", &json!({})).await;
    }
    let profile = ctx.profile.clone();
    ctx.store.creds_mut(&profile).token = None;
    ctx.store.save_credentials()?;
    render_message(
        ctx.output,
        &format!("Logged out of profile '{profile}'."),
        json!({ "ok": true }),
    )
}

async fn whoami(ctx: &RuntimeContext) -> Result<()> {
    let client = ctx.client()?;
    let me = client.get("/auth/whoami", &[]).await?;
    let mut table = Table::new(["FIELD", "VALUE"]);
    for key in ["id", "type", "role", "tenantId"] {
        table.push([key.to_string(), util::field(&me, key)]);
    }
    render(ctx.output, &me, &table)
}

fn token(ctx: &RuntimeContext) -> Result<()> {
    match &ctx.token {
        Some(t) => {
            println!("{t}");
            Ok(())
        }
        None => Err(CliError::NotAuthenticated(
            "no token stored for this profile".into(),
        )),
    }
}

fn prompt_line(prompt: &str) -> Result<String> {
    use std::io::Write;
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}
