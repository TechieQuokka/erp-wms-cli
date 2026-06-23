//! `wms report` — read-only reports (tenant-scoped; shipper sees own).

use crate::cli::ReportAction;
use crate::context::RuntimeContext;
use crate::error::Result;
use crate::output::render_auto;

pub async fn run(action: ReportAction, ctx: RuntimeContext) -> Result<()> {
    ctx.require_tenant()?;
    let client = ctx.client()?;
    let value = match action {
        ReportAction::Inventory { as_of } => {
            let mut q = Vec::new();
            if let Some(d) = as_of {
                q.push(("as_of", d));
            }
            client.get("/reports/inventory", &q).await?
        }
        ReportAction::Inbound { since, until } => {
            client
                .get("/reports/inbound", &window(since, until))
                .await?
        }
        ReportAction::Outbound { since, until } => {
            client
                .get("/reports/outbound", &window(since, until))
                .await?
        }
        ReportAction::Activity { date } => {
            // The activity report is windowed; a single --date is a one-day window.
            let q = match date {
                Some(d) => vec![("since", d.clone()), ("until", d)],
                None => vec![],
            };
            client.get("/reports/activity", &q).await?
        }
        ReportAction::BillingData { period } => {
            client
                .get("/reports/billing-data", &[("period", period)])
                .await?
        }
    };
    render_auto(ctx.output, &value)
}

fn window(since: Option<String>, until: Option<String>) -> Vec<(&'static str, String)> {
    let mut q = Vec::new();
    if let Some(d) = since {
        q.push(("since", d));
    }
    if let Some(d) = until {
        q.push(("until", d));
    }
    q
}
