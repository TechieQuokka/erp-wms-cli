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
    /// Backend liveness check (no auth).
    Health,
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
        /// Login email; prompted if omitted.
        #[arg(long)]
        email: Option<String>,
        /// Password; prompted securely if omitted.
        #[arg(long)]
        password: Option<String>,
    },
    /// Revoke the current session and clear the stored token.
    Logout,
    /// Show the current principal.
    Whoami,
    /// Print the current token (CI/scripting).
    Token,
    /// Change your own password. Revokes all other sessions and refreshes the
    /// stored token for this profile.
    ChangePassword {
        /// Current password; prompted securely if omitted.
        #[arg(long)]
        current_password: Option<String>,
        /// New password (min 8 chars); prompted securely (with confirmation) if omitted.
        #[arg(long)]
        new_password: Option<String>,
    },
}

// ---------------------------------------------------------------- config
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Set a setting (endpoint | default-output | default-tenant).
    Set {
        /// Setting key.
        key: String,
        /// Setting value.
        value: String,
    },
    /// Get a setting.
    Get {
        /// Setting key.
        key: String,
    },
    /// List settings for the active profile.
    List,
    /// Switch the default profile.
    Use {
        /// Profile name.
        profile: String,
    },
    /// List known profiles.
    Profiles,
}

// ---------------------------------------------------------------- tenant
#[derive(Debug, Subcommand)]
pub enum TenantAction {
    /// List shippers (admin+).
    List {
        /// Filter by status (default: active).
        #[arg(long, value_parser = ["active", "disabled", "all"])]
        status: Option<String>,
    },
    /// Show one shipper by code.
    Get {
        /// Tenant code (e.g. ACME).
        code: String,
    },
    /// Create a shipper (admin+).
    Create {
        /// Display name.
        #[arg(long)]
        name: String,
        /// Unique tenant code (e.g. ACME).
        #[arg(long)]
        code: String,
        /// Billing/contact email.
        #[arg(long)]
        contact_email: Option<String>,
        /// ISO-4217 currency (default: USD).
        #[arg(long)]
        currency: Option<String>,
    },
    /// Update a shipper's mutable fields (admin+).
    Update {
        /// Tenant code.
        code: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New contact email.
        #[arg(long)]
        contact_email: Option<String>,
        /// New default currency (ISO-4217).
        #[arg(long)]
        currency: Option<String>,
    },
    /// Disable a shipper (soft, reversible; admin+).
    Disable {
        /// Tenant code.
        code: String,
    },
    /// Re-enable a disabled shipper (admin+).
    Enable {
        /// Tenant code.
        code: String,
    },
}

// ---------------------------------------------------------------- user
#[derive(Debug, Subcommand)]
pub enum UserAction {
    /// List accounts (admin+).
    List {
        /// Restrict to one tenant's users.
        #[arg(long)]
        tenant: Option<String>,
        /// Filter by role.
        #[arg(long, value_parser = ["admin", "worker", "shipper"])]
        role: Option<String>,
    },
    /// Show one account by email or id.
    Get {
        /// User email or id.
        id: String,
    },
    /// Create a worker/shipper account (admin+).
    Create {
        /// Login email.
        #[arg(long)]
        email: String,
        /// Full name.
        #[arg(long)]
        name: String,
        /// Role to assign (admin cannot grant admin/developer).
        #[arg(long, value_parser = ["worker", "shipper"])]
        role: String,
        /// Tenant code (required for shipper accounts).
        #[arg(long)]
        tenant: Option<String>,
        /// Initial password; if omitted a one-time temp password is printed.
        #[arg(long)]
        password: Option<String>,
    },
    /// Update an account's name/email (admin+).
    Update {
        /// User email or id.
        id: String,
        /// New full name.
        #[arg(long)]
        name: Option<String>,
        /// New login email.
        #[arg(long)]
        email: Option<String>,
    },
    /// Change an account's role (admin+).
    SetRole {
        /// User email or id.
        id: String,
        /// New role.
        #[arg(long, value_parser = ["worker", "shipper"])]
        role: String,
    },
    /// Disable an account (admin+).
    Disable {
        /// User email or id.
        id: String,
    },
    /// Re-enable a disabled account (admin+).
    Enable {
        /// User email or id.
        id: String,
    },
    /// Reset a password; prints a one-time temp password (admin+).
    ResetPassword {
        /// User email or id.
        id: String,
    },
}

// ---------------------------------------------------------------- item
#[derive(Debug, Subcommand)]
pub enum ItemAction {
    /// List SKUs (worker+; requires --tenant for operators).
    List {
        /// Free-text search over sku/name/barcode.
        #[arg(long)]
        search: Option<String>,
        /// Filter by status (default: active).
        #[arg(long, value_parser = ["active", "all"])]
        status: Option<String>,
    },
    /// Show one SKU.
    Get {
        /// Item SKU.
        sku: String,
    },
    /// Create a SKU (admin+).
    Create {
        /// Unique SKU code.
        #[arg(long)]
        sku: String,
        /// Display name.
        #[arg(long)]
        name: String,
        /// Barcode / EAN.
        #[arg(long)]
        barcode: Option<String>,
        /// Unit of measure (default: ea).
        #[arg(long)]
        uom: Option<String>,
        /// Low-stock threshold for alerts.
        #[arg(long)]
        min_level: Option<i64>,
    },
    /// Update a SKU's mutable fields (admin+).
    Update {
        /// Item SKU.
        sku: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New barcode / EAN.
        #[arg(long)]
        barcode: Option<String>,
        /// New unit of measure.
        #[arg(long)]
        uom: Option<String>,
        /// New low-stock threshold.
        #[arg(long)]
        min_level: Option<i64>,
    },
    /// Bulk-import SKUs from a CSV file (admin+).
    Import {
        /// Path to the CSV file.
        file: std::path::PathBuf,
        /// Validate and report without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Disable a SKU (admin+).
    Disable {
        /// Item SKU.
        sku: String,
    },
}

// ---------------------------------------------------------------- location
#[derive(Debug, Subcommand)]
pub enum LocationAction {
    /// List locations (worker+).
    List {
        /// Filter by zone.
        #[arg(long)]
        zone: Option<String>,
        /// Filter by location type.
        #[arg(long, value_parser = ["storage", "picking", "receiving", "shipping", "staging"])]
        r#type: Option<String>,
        /// Only show empty (zero on-hand) locations.
        #[arg(long)]
        empty: bool,
    },
    /// Show one location.
    Get {
        /// Location code (e.g. A-01-03).
        code: String,
    },
    /// Create a location (admin+).
    Create {
        /// Unique location code (e.g. A-01-03).
        #[arg(long)]
        code: String,
        /// Zone label (e.g. A).
        #[arg(long)]
        zone: Option<String>,
        /// Location type (default: storage).
        #[arg(long, value_parser = ["storage", "picking", "receiving", "shipping", "staging"])]
        r#type: Option<String>,
        /// Max capacity (units).
        #[arg(long)]
        capacity: Option<i64>,
    },
    /// Update a location's mutable fields (admin+).
    Update {
        /// Location code.
        code: String,
        /// New zone label.
        #[arg(long)]
        zone: Option<String>,
        /// New location type.
        #[arg(long, value_parser = ["storage", "picking", "receiving", "shipping", "staging"])]
        r#type: Option<String>,
        /// New max capacity.
        #[arg(long)]
        capacity: Option<i64>,
    },
    /// Bulk-import locations from a CSV file (admin+).
    Import {
        /// Path to the CSV file.
        file: std::path::PathBuf,
        /// Validate and report without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Disable a location (admin+).
    Disable {
        /// Location code.
        code: String,
    },
}

// ---------------------------------------------------------------- inbound
#[derive(Debug, Subcommand)]
pub enum InboundAction {
    /// List inbound ASNs (worker+).
    List {
        /// Filter by status.
        #[arg(long, value_parser = ["pending", "received", "putaway", "closed"])]
        status: Option<String>,
    },
    /// Show one inbound ASN.
    Get {
        /// Inbound id.
        id: String,
    },
    /// Create an expected inbound (ASN) (worker+).
    Create {
        /// External reference (PO/ASN number).
        #[arg(long)]
        r#ref: Option<String>,
        /// Expected arrival date (YYYY-MM-DD).
        #[arg(long)]
        eta: Option<String>,
        /// Expected line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Receive against an ASN, or blind-receive with no ASN (worker+).
    Receive {
        /// ASN id; omit for a blind receive (then --tenant is required).
        id: Option<String>,
        /// Received line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
        /// Blind-receive shortcut: also put away to this location.
        #[arg(long)]
        putaway_to: Option<String>,
    },
    /// Put received stock away into locations (worker+).
    Putaway {
        /// Inbound id.
        id: String,
        /// Putaway line as `<sku>@<location>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Cancel an inbound (worker+).
    Cancel {
        /// Inbound id.
        id: String,
    },
}

// ---------------------------------------------------------------- inventory
#[derive(Debug, Subcommand)]
pub enum InventoryAction {
    /// List stock on hand (worker+).
    List {
        /// Filter by SKU.
        #[arg(long)]
        sku: Option<String>,
        /// Filter by location code.
        #[arg(long)]
        location: Option<String>,
        /// Filter by zone.
        #[arg(long)]
        zone: Option<String>,
        /// Only rows at or below this on-hand quantity.
        #[arg(long)]
        below: Option<i64>,
    },
    /// Show stock for one SKU across locations (worker+).
    Get {
        /// Item SKU.
        sku: String,
    },
    /// Move stock between locations (worker+).
    Move {
        /// Item SKU.
        #[arg(long)]
        sku: String,
        /// Source location code.
        #[arg(long)]
        from: String,
        /// Destination location code.
        #[arg(long)]
        to: String,
        /// Quantity to move.
        #[arg(long)]
        qty: i64,
    },
    /// Record a physical count at a location (worker+).
    Count {
        /// Location code being counted.
        #[arg(long)]
        location: String,
        /// Counted line as `<sku>:<counted>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Show the inventory ledger (append-only movements) (worker+).
    Ledger {
        /// Filter by SKU.
        #[arg(long)]
        sku: Option<String>,
        /// Filter by location code.
        #[arg(long)]
        location: Option<String>,
        /// Start date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        since: Option<String>,
        /// End date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        until: Option<String>,
    },
    /// Force a correction (admin+; audited). Guarded — needs --yes.
    #[command(allow_negative_numbers = true)]
    Adjust {
        /// Item SKU.
        #[arg(long)]
        sku: String,
        /// Location code.
        #[arg(long)]
        location: String,
        /// Relative change (e.g. -5); mutually exclusive with --set.
        #[arg(long, conflicts_with = "set")]
        qty: Option<i64>,
        /// Absolute target quantity; mutually exclusive with --qty.
        #[arg(long)]
        set: Option<i64>,
        /// Reason for the adjustment (recorded in the audit log).
        #[arg(long)]
        reason: String,
    },
}

// ---------------------------------------------------------------- order
#[derive(Debug, Subcommand)]
pub enum OrderAction {
    /// List outbound orders (worker+).
    List {
        /// Filter by status (new|allocated|picking|packed|shipped|backorder|cancelled).
        #[arg(long)]
        status: Option<String>,
    },
    /// Show one order with its lines (worker+).
    Get {
        /// Order id.
        id: String,
    },
    /// Create an outbound order (worker+ or API key).
    Create {
        /// External order reference.
        #[arg(long)]
        r#ref: Option<String>,
        /// Ship-to recipient.
        #[arg(long)]
        ship_to: Option<String>,
        /// Order line as `<sku>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Bulk-import orders from a CSV file (worker+).
    Import {
        /// Path to the CSV file (grouped by order_ref).
        file: std::path::PathBuf,
        /// Validate and report without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Allocate stock to an order; partial → backorder (worker+).
    Allocate {
        /// Order id.
        id: String,
    },
    /// Pick allocated lines (worker+).
    Pick {
        /// Order id.
        id: String,
        /// Picked line as `<sku>@<location>:<qty>` (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Pack a fully-picked order (worker+).
    Pack {
        /// Order id.
        id: String,
    },
    /// Ship an order: deduct stock + ledger + tracking (worker+).
    Ship {
        /// Order id.
        id: String,
        /// Carrier tracking number.
        #[arg(long)]
        tracking: Option<String>,
    },
    /// Cancel an order, releasing reservations (worker+).
    Cancel {
        /// Order id.
        id: String,
        /// Reason for cancellation.
        #[arg(long)]
        reason: String,
    },
}

// ---------------------------------------------------------------- report
#[derive(Debug, Subcommand)]
pub enum ReportAction {
    /// Inventory snapshot report (tenant-scoped).
    Inventory {
        /// Snapshot date (YYYY-MM-DD); default: now.
        #[arg(long)]
        as_of: Option<String>,
    },
    /// Inbound movement report (tenant-scoped).
    Inbound {
        /// Start date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        since: Option<String>,
        /// End date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        until: Option<String>,
    },
    /// Outbound movement report (tenant-scoped).
    Outbound {
        /// Start date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        since: Option<String>,
        /// End date (inclusive, YYYY-MM-DD).
        #[arg(long)]
        until: Option<String>,
    },
    /// Daily in/out activity series for dashboards (tenant-scoped).
    Activity {
        /// Day to report (YYYY-MM-DD); default: today.
        #[arg(long)]
        date: Option<String>,
    },
    /// Accrual billing data for a period; no invoice (admin+).
    BillingData {
        /// Billing period (YYYY-MM).
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
    Query {
        /// SQL statement (must be SELECT or WITH).
        sql: String,
    },
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
        /// Filter by audit action name.
        #[arg(long)]
        action: Option<String>,
    },
    /// Runtime bindings + row counts.
    Debug,
    /// Wipe ALL data and re-seed a bootstrap developer (test env only; guarded).
    Reset {
        /// Email for the re-seeded bootstrap developer.
        #[arg(long)]
        seed_email: String,
        /// Password for the re-seeded bootstrap developer.
        #[arg(long)]
        seed_password: String,
        /// Confirmation phrase (must be `RESET`); required for non-interactive runs.
        #[arg(long)]
        confirm: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum DevKeyAction {
    /// Mint a new API key (the secret is shown once).
    Create {
        /// Human label for the key.
        #[arg(long)]
        name: String,
        /// Scope/role granted to the key.
        #[arg(long)]
        role: String,
        /// Tenant code to scope the key to.
        #[arg(long)]
        tenant: Option<String>,
        /// Expiry date (YYYY-MM-DD); omit for non-expiring.
        #[arg(long)]
        expires: Option<String>,
    },
    /// List API keys (secrets are never shown).
    List,
    /// Revoke an API key by id.
    Revoke {
        /// API key id.
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DevUserAction {
    /// Grant admin/developer to a user (privilege escalation).
    Grant {
        /// User email or id.
        id: String,
        /// Role to grant.
        #[arg(long, value_parser = ["admin", "developer"])]
        role: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DevTenantAction {
    /// Permanently delete a tenant and all its data (guarded).
    Delete {
        /// Tenant code.
        code: String,
    },
}
