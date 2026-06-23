//! HTTP client for the WMS API (`api-contract.md`).
//!
//! Wraps `reqwest` with the contract's conventions: `Authorization: Bearer`,
//! `X-Tenant` operator scoping, the `{ error: { code, message, request_id } }`
//! envelope, and the `{ data, next_cursor }` list shape.

use std::path::Path;

use reqwest::Method;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{ApiError, CliError, Result};

const API_PREFIX: &str = "/api/v1";

#[derive(Debug, Clone)]
pub enum Auth {
    Token(String),
    ApiKey(String),
}

impl Auth {
    fn bearer(&self) -> &str {
        match self {
            Auth::Token(t) => t,
            Auth::ApiKey(k) => k,
        }
    }
}

pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    auth: Option<Auth>,
    tenant: Option<String>,
    verbose: bool,
}

/// One page of a list response.
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

impl ApiClient {
    pub fn new(
        endpoint: String,
        auth: Option<Auth>,
        tenant: Option<String>,
        verbose: bool,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("wms-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(CliError::from)?;
        Ok(ApiClient {
            http,
            base: normalize_base(&endpoint),
            auth,
            tenant,
            verbose,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn headers(&self, with_tenant: bool, idempotency: Option<&str>) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        if let Some(auth) = &self.auth {
            let val = format!("Bearer {}", auth.bearer());
            h.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&val)
                    .map_err(|_| CliError::Usage("invalid credential characters".into()))?,
            );
        }
        if with_tenant {
            if let Some(t) = &self.tenant {
                h.insert(
                    "x-tenant",
                    HeaderValue::from_str(t)
                        .map_err(|_| CliError::Usage("invalid tenant code".into()))?,
                );
            }
        }
        if let Some(key) = idempotency {
            h.insert("idempotency-key", HeaderValue::from_str(key).unwrap());
        }
        Ok(h)
    }

    /// Core request: sends `body` as JSON (when present) and decodes the response.
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Result<Value> {
        let url = self.url(path);
        if self.verbose {
            eprintln!("→ {method} {url}");
        }
        let mut req = self
            .http
            .request(method, &url)
            .headers(self.headers(true, idempotency)?);
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(CliError::from)?;
        decode(resp, self.verbose).await
    }

    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.send(Method::GET, path, query, None, None).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(
            Method::POST,
            path,
            &[],
            Some(body),
            Some(&new_idempotency_key()),
        )
        .await
    }

    /// POST without an idempotency key (state-transition actions that are not creates).
    pub async fn post_action(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::POST, path, &[], Some(body), None).await
    }

    pub async fn patch(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::PATCH, path, &[], Some(body), None).await
    }

    pub async fn delete(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.send(Method::DELETE, path, query, None, None).await
    }

    /// Multipart CSV upload to an `:import` endpoint; the file field is `file`.
    pub async fn upload_csv(&self, path: &str, file: &Path, dry_run: bool) -> Result<Value> {
        let bytes = std::fs::read(file)?;
        let filename = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload.csv".into());
        self.upload_csv_bytes(path, &filename, bytes, dry_run).await
    }

    /// Uploads CSV bytes directly (used by the order-import chunker).
    pub async fn upload_csv_bytes(
        &self,
        path: &str,
        filename: &str,
        bytes: Vec<u8>,
        dry_run: bool,
    ) -> Result<Value> {
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str("text/csv")
            .map_err(CliError::from)?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let url = self.url(path);
        if self.verbose {
            eprintln!("→ POST {url} (multipart)");
        }
        let mut req = self
            .http
            .post(&url)
            .headers(self.headers(true, None)?)
            .multipart(form);
        if dry_run {
            req = req.query(&[("dry_run", "true")]);
        }
        let resp = req.send().await.map_err(CliError::from)?;
        decode(resp, self.verbose).await
    }

    /// Fetches every page of a list endpoint, following `next_cursor`.
    pub async fn list_all<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut q = query.to_vec();
            if let Some(c) = &cursor {
                q.push(("cursor", c.clone()));
            }
            let page = self.get_page::<T>(path, &q).await?;
            out.extend(page.data);
            match page.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }
        Ok(out)
    }

    async fn get_page<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Page<T>> {
        let v = self.get(path, query).await?;
        let data = v.get("data").cloned().unwrap_or(Value::Array(vec![]));
        let parsed: Vec<T> = serde_json::from_value(data).map_err(anyhow::Error::new)?;
        let next = v
            .get("next_cursor")
            .and_then(|c| c.as_str())
            .map(str::to_string);
        Ok(Page {
            data: parsed,
            next_cursor: next,
        })
    }
}

fn normalize_base(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with(API_PREFIX) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{API_PREFIX}")
    }
}

fn new_idempotency_key() -> String {
    // A random-enough key for safe retries; uniqueness per attempt is sufficient.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("wms-cli-{nanos:x}")
}

async fn decode(resp: reqwest::Response, verbose: bool) -> Result<Value> {
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let text = resp.text().await.map_err(CliError::from)?;
    if verbose {
        eprintln!("← {} ({} bytes)", status, text.len());
    }

    if status.is_success() {
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&text)
            .map_err(|e| CliError::Other(anyhow::Error::new(e).context("decoding response body")));
    }

    // Error path — decode the contract's envelope, falling back to the raw body.
    let envelope: Option<Value> = serde_json::from_str(&text).ok();
    let err = envelope.as_ref().and_then(|v| v.get("error"));
    let code = err
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| default_code(status.as_u16()))
        .to_string();
    let message = err
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if text.trim().is_empty() {
                status
                    .canonical_reason()
                    .unwrap_or("request failed")
                    .to_string()
            } else {
                text.clone()
            }
        });
    let request_id = err
        .and_then(|e| e.get("request_id"))
        .and_then(|r| r.as_str())
        .map(str::to_string);

    Err(CliError::Api(ApiError {
        code,
        message,
        request_id,
        retry_after,
    }))
}

fn default_code(status: u16) -> &'static str {
    match status {
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        422 => "validation_error",
        429 => "rate_limited",
        500..=599 => "internal",
        _ => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_gets_api_prefix() {
        assert_eq!(
            normalize_base("https://h.example.com"),
            "https://h.example.com/api/v1"
        );
        assert_eq!(
            normalize_base("https://h.example.com/"),
            "https://h.example.com/api/v1"
        );
    }

    #[test]
    fn base_not_double_prefixed() {
        assert_eq!(
            normalize_base("https://h.example.com/api/v1"),
            "https://h.example.com/api/v1"
        );
        assert_eq!(
            normalize_base("https://h.example.com/api/v1/"),
            "https://h.example.com/api/v1"
        );
    }

    #[test]
    fn idempotency_keys_differ() {
        let a = new_idempotency_key();
        let b = new_idempotency_key();
        assert!(a.starts_with("wms-cli-"));
        assert_ne!(a, b);
    }

    #[test]
    fn default_code_maps_status() {
        assert_eq!(default_code(404), "not_found");
        assert_eq!(default_code(422), "validation_error");
        assert_eq!(default_code(503), "internal");
    }
}
