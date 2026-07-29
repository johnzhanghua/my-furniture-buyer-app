//! Client for the hackathon's upstream API.
//!
//! # Status: credentials wired, endpoints not yet implemented
//!
//! The Day 1 participant guide sits behind GitHub SSO, so the endpoint paths,
//! auth scheme, and payload shapes have not been confirmed. What is settled and
//! implemented here:
//!
//! - the user id and API key are read from the environment, never hardcoded;
//! - every outbound request carries them (see [`ExternalApiClient::authorized`]);
//! - upstream failures map to a single `ApiError::Upstream`, so a flaky
//!   dependency surfaces as `502` rather than leaking as a `500`.
//!
//! What is **not** settled: which resources move upstream. Once the spec is
//! known, add typed methods here (`fetch_products`, `submit_order`, …) built on
//! [`ExternalApiClient::get_json`] / [`post_json`]. The auth header *names* are
//! configurable, so a `Bearer`-style scheme is an `.env` change rather than a
//! code change.

// `get_json` / `post_json` are the building blocks the endpoint methods will be
// written against; they have no callers until the spec lands. Remove this once
// real endpoints exist.
#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::config::ExternalApiConfig;
use crate::error::ApiError;

#[derive(Clone)]
pub struct ExternalApiClient {
    http: reqwest::Client,
    config: ExternalApiConfig,
}

impl ExternalApiClient {
    pub fn new(config: ExternalApiConfig) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ApiError::Internal(format!("could not build HTTP client: {e}")))?;

        Ok(Self { http, config })
    }

    pub fn is_configured(&self) -> bool {
        self.config.is_configured()
    }

    /// Builds a request with the credentials attached. The single place
    /// upstream auth is applied.
    fn authorized(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, ApiError> {
        if !self.config.is_configured() {
            return Err(ApiError::Upstream(
                "the upstream API is not configured; set EXTERNAL_API_BASE_URL and EXTERNAL_API_KEY".into(),
            ));
        }

        let url = format!("{}/{}", self.config.base_url, path.trim_start_matches('/'));

        Ok(self
            .http
            .request(method, url)
            .header(&self.config.key_header, &self.config.api_key)
            .header(&self.config.user_header, &self.config.user_id))
    }

    /// Sends a credentialed GET and reports the raw status, without assuming
    /// anything about the response body. Used by `GET /api/upstream/status` to
    /// check reachability and credentials before the real endpoints are known.
    pub async fn probe(&self, path: &str) -> Result<u16, ApiError> {
        let response = self
            .authorized(reqwest::Method::GET, path)?
            .send()
            .await
            .map_err(|e| Self::transport_error(path, e))?;

        Ok(response.status().as_u16())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let response = self
            .authorized(reqwest::Method::GET, path)?
            .send()
            .await
            .map_err(|e| Self::transport_error(path, e))?;

        Self::decode(path, response).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let response = self
            .authorized(reqwest::Method::POST, path)?
            .json(body)
            .send()
            .await
            .map_err(|e| Self::transport_error(path, e))?;

        Self::decode(path, response).await
    }

    async fn decode<T: DeserializeOwned>(
        path: &str,
        response: reqwest::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();

        if !status.is_success() {
            // Upstream's own body is logged but not forwarded — it may name
            // internal hosts, and its error shape is not ours to promise.
            let body = response.text().await.unwrap_or_default();
            log::error!("upstream {path} returned {status}: {body}");
            return Err(ApiError::Upstream(format!(
                "the upstream API returned {status}"
            )));
        }

        response.json::<T>().await.map_err(|e| {
            log::error!("upstream {path} returned an unreadable body: {e}");
            ApiError::Upstream("the upstream API returned an unexpected response".into())
        })
    }

    fn transport_error(path: &str, e: reqwest::Error) -> ApiError {
        log::error!("upstream {path} request failed: {e}");
        if e.is_timeout() {
            ApiError::Upstream("the upstream API timed out".into())
        } else {
            ApiError::Upstream("could not reach the upstream API".into())
        }
    }
}
