use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use alloy::rpc::types::beacon::events::HeadEvent;
use futures::StreamExt;
use reqwest_eventsource::EventSource;
use tokio::{sync::broadcast::Sender, time::sleep};
use tracing::{debug, error, warn};
use url::Url;

/// Handles communication with multiple `BeaconClient` instances.
/// Load balances requests.
#[derive(Clone)]
pub struct MultiBeaconClient {
    /// Vec of all beacon clients with a fixed usize ID used when
    /// fetching: `beacon_clients_by_last_response`
    pub beacon_clients: Vec<(usize, Arc<BeaconClient>)>,
    /// The ID of the beacon client with the most recent successful response.
    pub best_beacon_instance: Arc<AtomicUsize>,
}

impl MultiBeaconClient {
    pub fn new(beacon_clients: Vec<Arc<BeaconClient>>) -> Self {
        let beacon_clients_with_index = beacon_clients.into_iter().enumerate().collect();

        Self {
            beacon_clients: beacon_clients_with_index,
            best_beacon_instance: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn from_endpoint_strs(endpoints: &[String]) -> Self {
        let clients = endpoints
            .iter()
            .map(|endpoint| Arc::new(BeaconClient::from_endpoint_str(endpoint)))
            .collect();
        Self::new(clients)
    }

    /// `subscribe_to_head_events` subscribes to head events from all beacon nodes.
    ///
    /// This function swaps async tasks for all beacon clients. Therefore,
    /// a single head event will be received multiple times, likely once for every beacon node.
    pub async fn subscribe_to_head_events(&self, chan: Sender<HeadEvent>) {
        let clients = self.beacon_clients_by_last_response();

        for (_, client) in clients {
            let chan = chan.clone();
            tokio::spawn(async move {
                client.subscribe_to_head_events(chan).await;
            });
        }
    }

    /// Returns a list of beacon clients, prioritized by the last successful response.
    ///
    /// The beacon client with the most recent successful response is placed at the
    /// beginning of the returned vector. All other clients maintain their original order.
    pub fn beacon_clients_by_last_response(&self) -> Vec<(usize, Arc<BeaconClient>)> {
        let mut instances = self.beacon_clients.clone();
        let index = self.best_beacon_instance.load(Ordering::Relaxed);
        if index != 0 {
            let pos = instances.iter().position(|(i, _)| *i == index).unwrap();
            instances.swap(0, pos);
        }
        instances
    }
}

/// Handles communication to a single beacon client url.
#[derive(Clone, Debug)]
pub struct BeaconClient {
    pub endpoint: Url,
}

impl BeaconClient {
    pub fn new(endpoint: Url) -> Self {
        Self { endpoint }
    }

    pub fn from_endpoint_str(endpoint: &str) -> Self {
        let endpoint = Url::parse(endpoint).unwrap();
        Self::new(endpoint)
    }

    async fn subscribe_to_head_events(&self, chan: Sender<HeadEvent>) {
        self.subscribe_to_sse("head", chan).await
    }

    /// Subscribe to SSE events from the beacon client `events` endpoint.
    pub async fn subscribe_to_sse<T: serde::de::DeserializeOwned>(
        &self,
        topic: &str,
        chan: Sender<T>,
    ) {
        let url = format!("{}eth/v1/events?topics={}", self.endpoint, topic);

        loop {
            let mut es = EventSource::get(&url);

            while let Some(event) = es.next().await {
                match event {
                    Ok(reqwest_eventsource::Event::Message(message)) => {
                        match serde_json::from_str::<T>(&message.data) {
                            Ok(data) => {
                                if chan.send(data).is_err() {
                                    debug!("no subscribers connected to sse broadcaster");
                                }
                            }
                            Err(err) => error!(err=%err, "Error parsing chunk"),
                        }
                    }
                    Ok(reqwest_eventsource::Event::Open) => {}
                    Err(err) => {
                        warn!(err=%err, "SSE stream ended, reconnecting...");
                        es.close();
                        break;
                    }
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
    }
}
