use std::{sync::Arc, time::Duration};

use futures_util::future::join_all;
use reqwest::{ClientBuilder, StatusCode};
use tracing::{error, trace};

use super::RelayClientConfig;
use crate::{
    constants::{EPOCH_SLOTS, GET_PRECONFERS_PATH, GET_PRECONFER_PATH},
    preconf::election::SignedPreconferElection,
    relay_client::error::RelayClientError,
};

const RELAY_CLIENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// `RelayClient` handles communication with a single relay.
#[derive(Clone, Debug)]
pub struct RelayClient {
    client: reqwest::Client,
    config: Arc<RelayClientConfig>,
}

impl RelayClient {
    /// Creates a new `RelayClient` instance.
    /// Initialises a reqwest Client with a 5-second timeout.
    pub fn new(config: Arc<RelayClientConfig>) -> Self {
        let client = ClientBuilder::new().timeout(RELAY_CLIENT_REQUEST_TIMEOUT).build().unwrap();
        Self { client, config }
    }

    /// Fetches elected preconfers for the entire epoch.
    ///
    /// If the relay supports lookahead, it uses it to fetch all preconfers at once.
    /// Otherwise, it fetches preconfers for each individual slot in the epoch.
    pub async fn get_elected_preconfers_for_epoch(
        &self,
        epoch: u64,
    ) -> Result<Option<Vec<SignedPreconferElection>>, RelayClientError> {
        if self.config.get_lookahead_enabled {
            return self.get_elected_preconfer_lookahead().await;
        }

        // Fetch the preconfer for each individual slot in the 32 slot epoch.
        let epoch_start_slot = epoch * EPOCH_SLOTS;
        let mut slot_handles = Vec::new();
        for i in 0..EPOCH_SLOTS {
            slot_handles.push(self.get_elected_preconfer_for_slot(epoch_start_slot + i));
        }

        let preconfer_elections: Vec<SignedPreconferElection> = join_all(slot_handles)
            .await
            .into_iter()
            .filter_map(|result| result.ok().flatten())
            .collect();

        if preconfer_elections.is_empty() {
            Ok(None)
        } else {
            Ok(Some(preconfer_elections))
        }
    }

    /// Fetches all elected preconfers from the current slot onwards from the relay.
    /// See spec: [https://www.notion.so/Aligning-Preconfirmation-APIs-db7907d9e66e41718e6bc2cff19604e4?pvs=4#21cd6f7f864d417b9d9727bd8c29fc6e].
    pub async fn get_elected_preconfer_lookahead(
        &self,
    ) -> Result<Option<Vec<SignedPreconferElection>>, RelayClientError> {
        let url = format!("{}{}", self.url(), GET_PRECONFERS_PATH);

        trace!(target: "lookahead", url, "fetching elected preconfers from relay");

        match self.client.get(url).send().await {
            Ok(result) => {
                trace!(target: "lookahead", status = ?result.status(), "fetched preconfer elections");

                if result.status() == StatusCode::NO_CONTENT {
                    trace!(target: "lookahead", "no elected preconfers found");
                    return Ok(None);
                }

                let preconfer_elections = result.json::<Vec<SignedPreconferElection>>().await?;

                trace!(target: "lookahead", "fetched {} elections", preconfer_elections.len());
                Ok(Some(preconfer_elections))
            }
            Err(err) => {
                error!(target: "lookahead", error = ?err, "failed to fetch preconfer elections");
                Err(RelayClientError::ReqwestError(err))
            }
        }
    }

    /// Fetches the elected preconfer for a specific slot.
    /// See spec: [https://www.notion.so/Aligning-Preconfirmation-APIs-db7907d9e66e41718e6bc2cff19604e4?pvs=4#21cd6f7f864d417b9d9727bd8c29fc6e].
    pub async fn get_elected_preconfer_for_slot(
        &self,
        slot: u64,
    ) -> Result<Option<SignedPreconferElection>, RelayClientError> {
        let url = format!("{}{}{}", self.url(), GET_PRECONFER_PATH, slot);

        let result = self.client.get(url).send().await?;
        if result.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let preconfer_election = result.json::<SignedPreconferElection>().await?;
        Ok(Some(preconfer_election))
    }

    /// Returns the URL of the relay.
    pub fn url(&self) -> &str {
        &self.config.url
    }
}
