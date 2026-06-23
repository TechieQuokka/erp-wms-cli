//! Runtime context: resolves the effective settings for one invocation by
//! applying the precedence `flag > WMS_* env > profile config > default`.

use crate::cli::GlobalArgs;
use crate::client::{ApiClient, Auth};
use crate::config::ConfigStore;
use crate::error::{CliError, Result};
use crate::output::OutputFormat;

pub struct RuntimeContext {
    pub store: ConfigStore,
    pub profile: String,
    pub endpoint: Option<String>,
    pub token: Option<String>,
    pub api_key: Option<String>,
    pub tenant: Option<String>,
    pub output: OutputFormat,
    pub assume_yes: bool,
    pub verbose: bool,
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

impl RuntimeContext {
    pub fn resolve(g: &GlobalArgs) -> Result<Self> {
        let store = ConfigStore::load()?;
        let profile = store.active_profile(g.profile.as_deref());
        let settings = store.settings(&profile);
        let creds = store.creds(&profile);

        let endpoint = g
            .endpoint
            .clone()
            .or_else(|| env_opt("WMS_ENDPOINT"))
            .or(settings.endpoint.clone());

        let token = g
            .token
            .clone()
            .or_else(|| env_opt("WMS_TOKEN"))
            .or(creds.token.clone());
        let api_key = g
            .api_key
            .clone()
            .or_else(|| env_opt("WMS_API_KEY"))
            .or(creds.api_key.clone());

        let tenant = g
            .tenant
            .clone()
            .or_else(|| env_opt("WMS_TENANT"))
            .or(settings.default_tenant.clone());

        let output = match g.output {
            Some(o) => o,
            None => settings
                .default_output
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
        };

        Ok(RuntimeContext {
            store,
            profile,
            endpoint,
            token,
            api_key,
            tenant,
            output,
            assume_yes: g.yes,
            verbose: g.verbose,
        })
    }

    fn endpoint(&self) -> Result<String> {
        self.endpoint.clone().ok_or_else(|| {
            CliError::Usage(
                "no API endpoint configured — set one with `wms config set endpoint <url>`, \
                 --endpoint, or WMS_ENDPOINT"
                    .into(),
            )
        })
    }

    fn auth(&self) -> Option<Auth> {
        if let Some(k) = &self.api_key {
            Some(Auth::ApiKey(k.clone()))
        } else {
            self.token.clone().map(Auth::Token)
        }
    }

    /// Builds a client that does not require credentials (used by `auth login`).
    pub fn client_anon(&self) -> Result<ApiClient> {
        ApiClient::new(
            self.endpoint()?,
            self.auth(),
            self.tenant.clone(),
            self.verbose,
        )
    }

    /// Builds a client and fails early if no credentials are available.
    pub fn client(&self) -> Result<ApiClient> {
        let auth = self.auth().ok_or_else(|| {
            CliError::NotAuthenticated(
                "run `wms auth login` or provide --token / --api-key (WMS_TOKEN / WMS_API_KEY)"
                    .into(),
            )
        })?;
        ApiClient::new(
            self.endpoint()?,
            Some(auth),
            self.tenant.clone(),
            self.verbose,
        )
    }

    /// Returns the working tenant or a usage error when one is required.
    pub fn require_tenant(&self) -> Result<String> {
        self.tenant.clone().ok_or_else(|| {
            CliError::Usage(
                "this command needs a tenant — pass --tenant <code>, set WMS_TENANT, or \
                 `wms config set default-tenant <code>`"
                    .into(),
            )
        })
    }
}
