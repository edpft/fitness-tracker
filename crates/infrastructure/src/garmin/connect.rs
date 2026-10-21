//! What every one of Garmin's metric walks asks through: a signed-in client,
//! and the account's name that some of its endpoints are addressed by.

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use application::SourceError;

use super::auth::GarminAuth;

/// Where the signed-in account's name is served from.
const PROFILE_PATH: &str = "/userprofile-service/socialProfile";

/// A signed-in client for the API host.
#[derive(Debug)]
pub(super) struct Connect {
    auth: Arc<GarminAuth>,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
    /// The account's name, asked for once a walk and only by an endpoint
    /// addressed by it.
    name: Mutex<Option<String>>,
}

impl Connect {
    pub(super) fn new(api_base: impl Into<String>, auth: impl Into<Arc<GarminAuth>>) -> Self {
        Self {
            auth: auth.into(),
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            client: OnceLock::new(),
            name: Mutex::new(None),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }

    /// The bytes of one answer, as served.
    pub(super) async fn get(
        &self,
        path: &str,
        query: &[(&'static str, String)],
    ) -> Result<Vec<u8>, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}{path}", self.api_base))
            .query(query)
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        super::answer_bytes(response, path).await
    }

    /// The signed-in account's name, which is not the email it signs in with.
    pub(super) async fn name(&self) -> Result<String, SourceError> {
        let held = self
            .name
            .lock()
            .map_err(|_| SourceError::Unavailable {
                detail: "the account name was poisoned by an earlier panic".to_owned(),
            })?
            .clone();
        if let Some(name) = held {
            return Ok(name);
        }
        let bytes = self.get(PROFILE_PATH, &[]).await?;
        let profile: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| SourceError::Malformed {
                detail: format!("{PROFILE_PATH} answered something that is not JSON: {error}"),
            })?;
        let name = profile
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| SourceError::Malformed {
                detail: format!("{PROFILE_PATH} names no account"),
            })?
            .to_owned();
        if let Ok(mut held) = self.name.lock() {
            *held = Some(name.clone());
        }
        Ok(name)
    }
}
