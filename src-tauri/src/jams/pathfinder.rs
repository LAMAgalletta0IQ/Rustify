use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use serde_json::{json, Value};
use tracing::debug;

use super::client_token::ClientTokenManager;
use super::config::JamConfig;
use super::error::JamError;
use super::token::TokenProvider;
use super::ConnectionId;

/// Client for Spotify's internal Pathfinder GraphQL gateway.
///
/// Pathfinder only accepts **persisted queries**: you must replay the exact
/// `operationName` + `sha256Hash` pair captured from the official client —
/// arbitrary GraphQL is rejected. Hashes change whenever Spotify ships a new
/// client, so they live in `JamConfig.pathfinder_hashes` (or can be registered
/// at runtime via [`PathfinderClient::register_hash`]) rather than in this
/// struct.
#[derive(Clone)]
pub struct PathfinderClient {
    http: Client,
    endpoint: String,
    connection_id: ConnectionId,
    access_token: Arc<dyn TokenProvider>,
    client_token: ClientTokenManager,
    hashes: HashMap<String, String>,
}

impl PathfinderClient {
    pub fn new(
        http: Client,
        config: &JamConfig,
        connection_id: ConnectionId,
        access_token: Arc<dyn TokenProvider>,
        client_token: ClientTokenManager,
    ) -> Self {
        Self {
            http,
            endpoint: config.pathfinder_url.clone(),
            connection_id,
            access_token,
            client_token,
            hashes: config.pathfinder_hashes.clone(),
        }
    }

    /// Registers or overrides the persisted-query hash for an operation at
    /// runtime — use this when a hash goes stale and you want to swap it
    /// without restarting with a new config file.
    pub fn register_hash(&mut self, operation: impl Into<String>, hash: impl Into<String>) {
        self.hashes.insert(operation.into(), hash.into());
    }

    /// Runs a persisted query, taking the hash explicitly. The hash is trusted
    /// to be a capture from the official client.
    pub async fn query(
        &self,
        operation_name: &str,
        variables: Value,
        hash: &str,
    ) -> Result<Value, JamError> {
        let token = self.access_token.access_token().ok_or_else(|| {
            JamError::ClientTokenExpired(
                "no OAuth access token; authenticate before querying Pathfinder".into(),
            )
        })?;
        let client_token = self.client_token.get_token().await?;

        let payload = json!({
            "operationName": operation_name,
            "variables": variables,
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash,
                }
            }
        });

        let mut req = self
            .http
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .header("client-token", client_token);
        let conn_id = self.connection_id.current();
        if !conn_id.is_empty() {
            req = req.header("Spotify-Connection-Id", conn_id);
        }
        let resp = req.json(&payload).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(JamError::PersistedQueryExpired {
                operation: operation_name.to_owned(),
                hash: hash.to_owned(),
                message: format!("HTTP {status}: {text}"),
            });
        }

        let v: Value = serde_json::from_str(&text)?;
        if let Some(errors) = v.get("errors").and_then(|e| e.as_array()).filter(|e| !e.is_empty()) {
            return Err(JamError::PersistedQueryExpired {
                operation: operation_name.to_owned(),
                hash: hash.to_owned(),
                message: format!("{errors:?}"),
            });
        }

        debug!("pathfinder query {operation_name} succeeded");
        Ok(v.get("data").cloned().unwrap_or(Value::Null))
    }

    /// Runs a persisted query using the hash registered in the config under the
    /// operation name — the common case for calls wired to a known operation.
    pub async fn query_registered(
        &self,
        operation_name: &str,
        variables: Value,
    ) -> Result<Value, JamError> {
        let hash = self.hashes.get(operation_name).ok_or_else(|| JamError::PersistedQueryExpired {
            operation: operation_name.to_owned(),
            hash: "<unregistered>".to_owned(),
            message:
                "no sha256 hash is registered for this operation; capture it from the official \
                 client and add it to JamConfig.pathfinder_hashes"
                    .into(),
        })?;
        self.query(operation_name, variables, hash).await
    }
}