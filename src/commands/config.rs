//! `wms config` — local settings & profiles (no API calls).

use serde_json::json;

use crate::cli::ConfigAction;
use crate::context::RuntimeContext;
use crate::error::{CliError, Result};
use crate::output::{Table, render, render_message};

pub fn run(action: ConfigAction, mut ctx: RuntimeContext) -> Result<()> {
    match action {
        ConfigAction::Set { key, value } => set(&mut ctx, &key, &value),
        ConfigAction::Get { key } => get(&ctx, &key),
        ConfigAction::List => list(&ctx),
        ConfigAction::Use { profile } => use_profile(&mut ctx, &profile),
        ConfigAction::Profiles => profiles(&ctx),
    }
}

fn set(ctx: &mut RuntimeContext, key: &str, value: &str) -> Result<()> {
    let profile = ctx.profile.clone();
    let s = ctx.store.settings_mut(&profile);
    match normalize_key(key) {
        "endpoint" => s.endpoint = Some(value.to_string()),
        "default-output" => {
            value
                .parse::<crate::output::OutputFormat>()
                .map_err(CliError::Usage)?;
            s.default_output = Some(value.to_string());
        }
        "default-tenant" => s.default_tenant = Some(value.to_string()),
        other => {
            return Err(CliError::Usage(format!(
                "unknown setting '{other}' (endpoint | default-output | default-tenant)"
            )));
        }
    }
    ctx.store.save_config()?;
    render_message(
        ctx.output,
        &format!("Set {key} = {value} (profile '{profile}')."),
        json!({ "ok": true, "profile": profile, "key": key, "value": value }),
    )
}

fn get(ctx: &RuntimeContext, key: &str) -> Result<()> {
    let s = ctx.store.settings(&ctx.profile);
    let value = match normalize_key(key) {
        "endpoint" => s.endpoint,
        "default-output" => s.default_output,
        "default-tenant" => s.default_tenant,
        other => return Err(CliError::Usage(format!("unknown setting '{other}'"))),
    };
    match value {
        Some(v) => render_message(ctx.output, &v, json!({ "key": key, "value": v })),
        None => render_message(ctx.output, "(unset)", json!({ "key": key, "value": null })),
    }
}

fn list(ctx: &RuntimeContext) -> Result<()> {
    let s = ctx.store.settings(&ctx.profile);
    let json = json!({
        "profile": ctx.profile,
        "endpoint": s.endpoint,
        "default-output": s.default_output,
        "default-tenant": s.default_tenant,
    });
    let mut table = Table::new(["KEY", "VALUE"]);
    table.push(["profile".to_string(), ctx.profile.clone()]);
    table.push([
        "endpoint".to_string(),
        s.endpoint.unwrap_or_else(|| "-".into()),
    ]);
    table.push([
        "default-output".to_string(),
        s.default_output.unwrap_or_else(|| "-".into()),
    ]);
    table.push([
        "default-tenant".to_string(),
        s.default_tenant.unwrap_or_else(|| "-".into()),
    ]);
    render(ctx.output, &json, &table)
}

fn use_profile(ctx: &mut RuntimeContext, profile: &str) -> Result<()> {
    ctx.store.set_default_profile(profile);
    // Ensure the profile section exists so it shows up in `config profiles`.
    let _ = ctx.store.settings_mut(profile);
    ctx.store.save_config()?;
    render_message(
        ctx.output,
        &format!("Default profile is now '{profile}'."),
        json!({ "ok": true, "default_profile": profile }),
    )
}

fn profiles(ctx: &RuntimeContext) -> Result<()> {
    let names: Vec<String> = ctx.store.config.profiles.keys().cloned().collect();
    let default = ctx.store.active_profile(None);
    let json = json!({ "default_profile": default, "profiles": names });
    let mut table = Table::new(["PROFILE", "DEFAULT"]);
    if names.is_empty() {
        table.push([default.clone(), "*".to_string()]);
    }
    for name in &names {
        let marker = if *name == default { "*" } else { "" };
        table.push([name.clone(), marker.to_string()]);
    }
    render(ctx.output, &json, &table)
}

fn normalize_key(key: &str) -> &str {
    match key {
        "default_output" => "default-output",
        "default_tenant" => "default-tenant",
        other => other,
    }
}
