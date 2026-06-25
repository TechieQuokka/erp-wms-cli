//! Command dispatch — routes a parsed `Command` to its group handler.

pub mod alert;
pub mod auth;
pub mod config;
pub mod dev;
pub mod health;
pub mod inbound;
pub mod inventory;
pub mod item;
pub mod location;
pub mod order;
pub mod report;
pub mod tenant;
pub mod user;

use crate::cli::Command;
use crate::context::RuntimeContext;
use crate::error::Result;

pub async fn dispatch(command: Command, ctx: RuntimeContext) -> Result<()> {
    match command {
        Command::Health => health::run(ctx).await,
        Command::Auth { action } => auth::run(action, ctx).await,
        Command::Config { action } => config::run(action, ctx),
        Command::Tenant { action } => tenant::run(action, ctx).await,
        Command::User { action } => user::run(action, ctx).await,
        Command::Item { action } => item::run(action, ctx).await,
        Command::Location { action } => location::run(action, ctx).await,
        Command::Inbound { action } => inbound::run(action, ctx).await,
        Command::Inventory { action } => inventory::run(action, ctx).await,
        Command::Order { action } => order::run(action, ctx).await,
        Command::Report { action } => report::run(action, ctx).await,
        Command::Alert { action } => alert::run(action, ctx).await,
        Command::Dev { action } => dev::run(action, ctx).await,
    }
}
