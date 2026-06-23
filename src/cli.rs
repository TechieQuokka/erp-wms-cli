//! Command-line surface (clap derive). Mirrors `cli-spec.md` §3.

use clap::{Args, Parser, Subcommand};

use crate::output::OutputFormat;

#[derive(Debug, Parser)]
#[command(
    name = "wms",
    version,
    about = "Official CLI for the 3PL WMS headless API",
    propagate_version = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

/// Flags available on every command (`cli-spec.md` §2).
#[derive(Debug, Args)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Named profile to use (default: "default").
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Backend API base URL.
    #[arg(long, global = true)]
    pub endpoint: Option<String>,

    /// Human session token (overrides the stored profile).
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// Machine API key (overrides WMS_API_KEY).
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    /// Working tenant context (operators).
    #[arg(long, global = true)]
    pub tenant: Option<String>,

    /// Output format.
    #[arg(long, global = true, value_enum)]
    pub output: Option<OutputFormat>,

    /// Skip interactive confirmation for guarded commands.
    #[arg(long, global = true)]
    pub yes: bool,

    /// Verbose (log requests to stderr).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Quiet (suppress non-essential output).
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authentication.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// CLI settings & profiles.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Shippers (tenants).
    Tenant {
        #[command(subcommand)]
        action: TenantAction,
    },
    /// User accounts.
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// SKUs / items.
    Item {
        #[command(subcommand)]
        action: ItemAction,
    },
    /// Warehouse locations.
    Location {
        #[command(subcommand)]
        action: LocationAction,
    },
    /// Inbound / receiving.
    Inbound {
        #[command(subcommand)]
        action: InboundAction,
    },
    /// Inventory / stock.
    Inventory {
        #[command(subcommand)]
        action: InventoryAction,
    },
    /// Outbound orders.
    Order {
        #[command(subcommand)]
        action: OrderAction,
    },
    /// Reports.
    Report {
        #[command(subcommand)]
        action: ReportAction,
    },
    /// Dashboard alerts.
    Alert {
        #[command(subcommand)]
        action: AlertAction,
    },
    /// Developer / system layer.
    Dev {
        #[command(subcommand)]
        action: DevAction,
    },
}

// ---------------------------------------------------------------- auth
#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Log in with email + password and store the session token.
    Login {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
    /// Revoke the current session and clear the stored token.
    Logout,
    /// Show the current principal.
    Whoami,
    /// Print the current token (CI/scripting).
    Token,
}

// ---------------------------------------------------------------- config
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Set a setting (endpoint | default-output | default-tenant).
    Set { key: String, value: String },
    /// Get a setting.
    Get { key: String },
    /// List settings for the active profile.
    List,
    /// Switch the default profile.
    Use { profile: String },
    /// List known profiles.
    Profiles,
}

// ---------------------------------------------------------------- tenant
#[derive(Debug, Subcommand)]
pub enum TenantAction {
    List {
        #[arg(long, value_parser = ["active", "disabled", "all"])]
        status: Option<String>,
    },
    Get {
        code: String,
    },
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        code: String,
        #[arg(long)]
        contact_email: Option<String>,
        #[arg(long)]
        currency: Option<String>,
    },
    Update {
        code: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        contact_email: Option<String>,
        #[arg(long)]
        currency: Option<String>,
    },
    Disable {
        code: String,
    },
    Enable {
        code: String,
    },
}

// ---------------------------------------------------------------- user
#[derive(Debug, Subcommand)]
pub enum UserAction {
    List {
        #[arg(long)]
        tenant: Option<String>,
        #[arg(long, value_parser = ["admin", "worker", "shipper"])]
        role: Option<String>,
    },
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
        #[arg(long, value_parser = ["worker", "shipper"])]
        role: String,
        #[arg(long)]
        tenant: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        email: Option<String>,
    },
    SetRole {
        id: String,
        #[arg(long, value_parser = ["worker", "shipper"])]
        role: String,
    },
    Disable {
        id: String,
    },
    Enable {
        id: String,
    },
    ResetPassword {
        id: String,
    },
}

// ---------------------------------------------------------------- item
#[derive(Debug, Subcommand)]
pub enum ItemAction {
    List {
        #[arg(long)]
        search: Option<String>,
        #[arg(long, value_parser = ["active", "all"])]
        status: Option<String>,
    },
    Get {
        sku: String,
    },
    Create {
        #[arg(long)]
        sku: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        barcode: Option<String>,
        #[arg(long)]
        uom: Option<String>,
        #[arg(long)]
        min_level: Option<i64>,
    },
    Update {
        sku: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        barcode: Option<String>,
        #[arg(long)]
        uom: Option<String>,
        #[arg(long)]
        min_level: Option<i64>,
    },
    Import {
        file: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Disable {
        sku: String,
    },
}

// ---------------------------------------------------------------- location
#[derive(Debug, Subcommand)]
pub enum LocationAction {
    List {
        #[arg(long)]
        zone: Option<String>,
        #[arg(long, value_parser = ["storage", "picking", "receiving", "shipping", "staging"])]
        r#type: Option<String>,
        #[arg(long)]
        empty: bool,
    },
    Get {
        code: String,
    },
    Create {
        #[arg(long)]
        code: String,
        #[arg(long)]
        zone: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        capacity: Option<i64>,
    },
    Update {
        code: String,
        #[arg(long)]
        zone: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        capacity: Option<i64>,
    },
    Import {
        file: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Disable {
        code: String,
    },
}

// ---------------------------------------------------------------- inbound
#[derive(Debug, Subcommand)]
pub enum InboundAction {
    List {
        #[arg(long, value_parser = ["pending", "received", "putaway", "closed"])]
        status: Option<String>,
    },
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        r#ref: Option<String>,
        #[arg(long)]
        eta: Option<String>,
        /// Line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    Receive {
        /// ASN id; omit for a blind receive (then --tenant is required).
        id: Option<String>,
        /// Line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
        /// Blind-receive shortcut: also put away to this location.
        #[arg(long)]
        putaway_to: Option<String>,
    },
    Putaway {
        id: String,
        /// Line as `<sku>@<location>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    Cancel {
        id: String,
    },
}

// ---------------------------------------------------------------- inventory
#[derive(Debug, Subcommand)]
pub enum InventoryAction {
    List {
        #[arg(long)]
        sku: Option<String>,
        #[arg(long)]
        location: Option<String>,
        #[arg(long)]
        zone: Option<String>,
        #[arg(long)]
        below: Option<i64>,
    },
    Get {
        sku: String,
    },
    Move {
        #[arg(long)]
        sku: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        qty: i64,
    },
    Count {
        #[arg(long)]
        location: String,
        /// Line as `<sku>:<counted>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    Ledger {
        #[arg(long)]
        sku: Option<String>,
        #[arg(long)]
        location: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
    },
    /// Force a correction (admin; audited). Guarded — needs --yes.
    #[command(allow_negative_numbers = true)]
    Adjust {
        #[arg(long)]
        sku: String,
        #[arg(long)]
        location: String,
        #[arg(long, conflicts_with = "set")]
        qty: Option<i64>,
        #[arg(long)]
        set: Option<i64>,
        #[arg(long)]
        reason: String,
    },
}

// ---------------------------------------------------------------- order
#[derive(Debug, Subcommand)]
pub enum OrderAction {
    List {
        #[arg(long)]
        status: Option<String>,
    },
    Get {
        id: String,
    },
    Create {
        #[arg(long)]
        r#ref: Option<String>,
        #[arg(long)]
        ship_to: Option<String>,
        /// Line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    Import {
        file: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Allocate {
        id: String,
    },
    Pick {
        id: String,
        /// Line as `<sku>@<location>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    Pack {
        id: String,
    },
    Ship {
        id: String,
        #[arg(long)]
        tracking: Option<String>,
    },
    Cancel {
        id: String,
        #[arg(long)]
        reason: String,
    },
}

// ---------------------------------------------------------------- report
#[derive(Debug, Subcommand)]
pub enum ReportAction {
    Inventory {
        #[arg(long)]
        as_of: Option<String>,
    },
    Inbound {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
    },
    Outbound {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
    },
    Activity {
        #[arg(long)]
        date: Option<String>,
    },
    BillingData {
        #[arg(long)]
        period: String,
    },
}

// ---------------------------------------------------------------- alert
#[derive(Debug, Subcommand)]
pub enum AlertAction {
    /// Show low-stock and unprocessed-order alerts.
    List,
}

// ---------------------------------------------------------------- dev
#[derive(Debug, Subcommand)]
pub enum DevAction {
    /// API keys.
    Key {
        #[command(subcommand)]
        action: DevKeyAction,
    },
    /// Read-only SQL (SELECT/WITH).
    Query { sql: String },
    /// Grant admin/developer (privilege escalation).
    User {
        #[command(subcommand)]
        action: DevUserAction,
    },
    /// Hard-delete a tenant (guarded).
    Tenant {
        #[command(subcommand)]
        action: DevTenantAction,
    },
    /// Read the audit log.
    Audit {
        #[arg(long)]
        action: Option<String>,
    },
    /// Runtime bindings + row counts.
    Debug,
}

#[derive(Debug, Subcommand)]
pub enum DevKeyAction {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        tenant: Option<String>,
        #[arg(long)]
        expires: Option<String>,
    },
    List,
    Revoke {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DevUserAction {
    Grant {
        id: String,
        #[arg(long, value_parser = ["admin", "developer"])]
        role: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DevTenantAction {
    Delete { code: String },
}
