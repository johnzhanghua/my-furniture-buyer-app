//! Shared library behind both binaries.
//!
//! - `furniture-buyer-api` (src/main.rs) — the HTTP API the SPA talks to.
//! - `furniture-mcp` (src/bin/mcp.rs) — MCP server exposing the furniture shop
//!   as agent tools.
//!
//! Both drive the same [`external_api::ExternalApiClient`], so upstream calls,
//! money conversion, and error mapping exist in exactly one place.

pub mod agent;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod external_api;
pub mod models;
pub mod routes;
pub mod state;
pub mod tools;
