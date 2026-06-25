//! `wms health` — backend liveness check (no auth, no tenant).

use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::{Table, render};
use crate::util::field;

pub async fn run(ctx: RuntimeContext) -> Result<()> {
    // Public liveness endpoint — no credentials or tenant context required.
    let client = ctx.client_anon()?;
    let v = client.get("/health", &[]).await?;

    let mut table = Table::new(["FIELD", "VALUE"]);
    for key in ["status", "service", "time"] {
        table.push([key.to_string(), field(&v, key)]);
    }
    render(ctx.output, &v, &table)
}
