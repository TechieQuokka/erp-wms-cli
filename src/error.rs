//! Error type and process exit codes.
//!
//! Exit codes follow `cli-spec.md` §5:
//! `0` success · `1` generic · `2` usage · `3` auth/forbidden · `4` not found ·
//! `5` validation · `6` conflict · `7` rate-limited.

use std::fmt;

/// A structured API error decoded from the `{ error: { code, message, .. } }`
/// envelope (`api-contract.md` §1).
#[derive(Debug, Clone)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    pub retry_after: Option<String>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)?;
        if let Some(secs) = &self.retry_after {
            write!(f, " — retry after {secs}s")?;
        }
        if let Some(id) = &self.request_id {
            write!(f, " [request_id={id}]")?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A bad invocation the user can fix (missing config, unknown flag combo).
    #[error("{0}")]
    Usage(String),

    /// No usable credentials / endpoint configured.
    #[error("not authenticated: {0}")]
    NotAuthenticated(String),

    /// The server returned a non-2xx response with an error envelope.
    #[error("{0}")]
    Api(ApiError),

    /// Transport / IO / serialization failure.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CliError {
    /// Maps the error to the process exit code defined by the CLI spec.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::NotAuthenticated(_) => 3,
            CliError::Other(_) => 1,
            CliError::Api(e) => match e.code.as_str() {
                "unauthorized" | "forbidden" => 3,
                "not_found" => 4,
                "validation_error" => 5,
                "conflict" => 6,
                "rate_limited" => 7,
                _ => 1,
            },
        }
    }
}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        CliError::Other(anyhow::Error::new(e))
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Other(anyhow::Error::new(e))
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn api(code: &str) -> CliError {
        CliError::Api(ApiError {
            code: code.into(),
            message: "m".into(),
            request_id: None,
            retry_after: None,
        })
    }

    #[test]
    fn exit_codes_match_spec() {
        assert_eq!(CliError::Usage("x".into()).exit_code(), 2);
        assert_eq!(CliError::NotAuthenticated("x".into()).exit_code(), 3);
        assert_eq!(api("unauthorized").exit_code(), 3);
        assert_eq!(api("forbidden").exit_code(), 3);
        assert_eq!(api("not_found").exit_code(), 4);
        assert_eq!(api("validation_error").exit_code(), 5);
        assert_eq!(api("conflict").exit_code(), 6);
        assert_eq!(api("rate_limited").exit_code(), 7);
        assert_eq!(api("internal").exit_code(), 1);
    }

    #[test]
    fn display_includes_retry_after() {
        let e = CliError::Api(ApiError {
            code: "rate_limited".into(),
            message: "slow".into(),
            request_id: None,
            retry_after: Some("30".into()),
        });
        assert!(e.to_string().contains("retry after 30s"));
    }
}
