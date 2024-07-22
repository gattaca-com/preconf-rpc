use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::post,
    Router,
};
use bytes::Bytes;
use eyre::{Context, Result};
use hashbrown::HashMap;
use http::Extensions;
use reqwest::{Request, Response, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{
    default_on_request_end, reqwest_otel_span, ReqwestOtelSpanBackend, TracingMiddleware,
};
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::Span;
use url::Url;

use crate::lookahead::LookaheadManager;

#[derive(Debug)]
pub(crate) struct SharedState {
    managers: HashMap<u16, LookaheadManager>,
    client: ClientWithMiddleware,
}

pub(crate) struct RpcForward {
    addr: String,
    shared_state: SharedState,
}

struct TimeTrace;

impl ReqwestOtelSpanBackend for TimeTrace {
    fn on_request_start(req: &Request, extension: &mut Extensions) -> Span {
        extension.insert(Instant::now());
        reqwest_otel_span!(
            level = tracing::Level::DEBUG,
            name = "reqwest-http-request",
            req,
            time_elapsed = tracing::field::Empty
        )
    }

    fn on_request_end(
        span: &Span,
        outcome: &reqwest_middleware::Result<Response>,
        extension: &mut Extensions,
    ) {
        let time_elapsed = extension.get::<Instant>().unwrap().elapsed().as_millis() as i64;
        default_on_request_end(span, outcome);
        span.record("time_elapsed", time_elapsed);
    }
}

impl SharedState {
    pub fn new(mut managers: HashMap<u16, LookaheadManager>) -> Result<Self> {
        // start lookahead provider for each manager
        for (_, manager) in managers.iter_mut() {
            manager.run_provider()?;
        }
        Ok(Self {
            managers,
            client: ClientBuilder::new(
                reqwest::ClientBuilder::new().timeout(Duration::from_secs(10)).build()?,
            )
            .with(TracingMiddleware::<TimeTrace>::new())
            .build(),
        })
    }
}

impl RpcForward {
    pub fn new(shared_state: SharedState, addr: String) -> Self {
        Self { addr, shared_state }
    }

    pub async fn start_service(self) -> Result<JoinHandle<Result<()>>> {
        let app = router(self.shared_state);
        let listener =
            tokio::net::TcpListener::bind(self.addr).await.wrap_err("failed to bind listener")?;
        Ok(tokio::spawn(async move {
            axum::serve(listener, app).await?;
            Ok(())
        }))
    }
}

fn router(shared_state: SharedState) -> Router {
    Router::new()
        .route("/:chain_id", post(scan_id_forward_request))
        .route("/", post(forward_request))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(shared_state))
}

#[tracing::instrument]
async fn scan_id_forward_request(
    State(state): State<Arc<SharedState>>,
    Path(chain_id): Path<u16>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if let Some(manager) = state.managers.get(&chain_id) {
        match manager.get_url() {
            Ok(url) => match inner_forward_request(&state.client, url, body, headers).await {
                Ok(res) => Ok(res),
                Err(_) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "error while forwarding request".to_string(),
                )),
            },
            Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
        }
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("no lookahead provider found for chain-id {}", chain_id),
        ))
    }
}

async fn forward_request(State(_state): State<Arc<SharedState>>) -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, "missing chain-id parameter")
}

async fn inner_forward_request(
    client: &ClientWithMiddleware,
    to_addr: Url,
    bytes: Bytes,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    let res = client.post(to_addr).body(bytes).headers(headers).send().await?;
    let body = res.bytes().await?;
    Ok(body)
}

#[cfg(test)]
mod test {
    use std::{
        default::Default,
        str::FromStr,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use alloy::rpc::types::beacon::{constants::BLS_PUBLIC_KEY_BYTES_LEN, BlsPublicKey};
    use axum::{
        extract::State,
        http::HeaderMap,
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use bytes::Bytes;
    use dashmap::DashMap;
    use eyre::Result;
    use hashbrown::HashMap;
    use http::{HeaderValue, StatusCode};
    use tokio::task::JoinHandle;
    use url::Url;

    use crate::{
        forward_service::{router, SharedState},
        lookahead::{Lookahead, LookaheadEntry, LookaheadManager, LookaheadProvider, UrlProvider},
        preconf::election::{PreconferElection, SignedPreconferElection},
    };

    struct DummySharedState {
        cnt: i32,
    }
    #[derive(Default)]
    struct TestBuilder {
        managers: Option<HashMap<u16, LookaheadManager>>,
        test_service: Option<u16>,
        forward_service: u16,
    }

    struct BuilderOutput {
        _fwd_service: Option<JoinHandle<()>>,
        _test_service: Option<JoinHandle<()>>,
    }

    impl TestBuilder {
        async fn build(self) -> Result<BuilderOutput> {
            let fwd_service = match self.managers {
                None => None,
                Some(managers) => Some(tokio::spawn(async move {
                    let router = router(SharedState::new(managers).unwrap());
                    let listener = tokio::net::TcpListener::bind(format!(
                        "localhost:{}",
                        self.forward_service
                    ))
                    .await
                    .unwrap();
                    axum::serve(listener, router).await.unwrap();
                })),
            };

            let test_service = match self.test_service {
                None => None,
                Some(port) => Some(tokio::spawn(async move {
                    let dst = Arc::new(Mutex::new(DummySharedState { cnt: 0 }));
                    let router: Router = Router::new()
                        .route("/", post(handle_request))
                        .route("/cnt", get(counter))
                        .with_state(dst);
                    let listener =
                        tokio::net::TcpListener::bind(format!("localhost:{}", port)).await.unwrap();
                    axum::serve(listener, router).await.unwrap();
                })),
            };
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(BuilderOutput { _fwd_service: fwd_service, _test_service: test_service })
        }
    }

    #[tokio::test]
    async fn test_missing_chain_id() -> Result<()> {
        let manager: LookaheadManager = Default::default();
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers =
            TestBuilder { managers: Some(managers), test_service: None, forward_service: 12001 }
                .build()
                .await?;
        let res = reqwest::Client::new().post("http://localhost:12001").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.text().await.unwrap(), "missing chain-id parameter");
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_chain_id() -> Result<()> {
        let manager: LookaheadManager = Default::default();
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers =
            TestBuilder { managers: Some(managers), test_service: None, forward_service: 12002 }
                .build()
                .await?;
        let res = reqwest::Client::new().post("http://localhost:12002/2").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.text().await.unwrap(), "no lookahead provider found for chain-id 2");
        Ok(())
    }

    #[tokio::test]
    async fn test_unavailable_forwarded_service() -> Result<()> {
        let map = Arc::new(DashMap::new());
        map.insert(0, LookaheadEntry {
            url: "http://not-a-valid-url".into(),
            ..Default::default()
        });
        let manager = LookaheadManager::new(
            Lookahead { map },
            LookaheadProvider::None,
            UrlProvider::LookaheadEntry,
        );
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers =
            TestBuilder { managers: Some(managers), test_service: None, forward_service: 12003 }
                .build()
                .await?;
        let res = reqwest::Client::new().post("http://localhost:12003/1").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        Ok(())
    }

    #[tokio::test]
    async fn test_forward_request() -> Result<()> {
        let map = Arc::new(DashMap::new());
        map.insert(0, LookaheadEntry {
            url: "http://localhost:12004".into(),
            ..Default::default()
        });
        let manager = LookaheadManager::new(
            Lookahead { map },
            LookaheadProvider::None,
            UrlProvider::LookaheadEntry,
        );
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers = TestBuilder {
            managers: Some(managers),
            test_service: Some(12004),
            forward_service: 12005,
        }
        .build()
        .await?;

        batch_requests(12005, 10).await?;

        let cnt_res = reqwest::get("http://localhost:12004/cnt").await.unwrap();
        assert_eq!(StatusCode::OK, cnt_res.status());
        assert_eq!(cnt_res.text().await.unwrap(), "10");
        Ok(())
    }

    #[tokio::test]
    async fn test_url_map_request() -> Result<()> {
        let map = Arc::new(DashMap::new());
        let signature: BlsPublicKey = BlsPublicKey::from([42u8; BLS_PUBLIC_KEY_BYTES_LEN]);
        let mut url_mapping = HashMap::new();
        url_mapping.insert(signature, Url::from_str("http://localhost:12006").unwrap());
        map.insert(0, LookaheadEntry {
            url: "".into(),
            election: SignedPreconferElection {
                message: PreconferElection {
                    preconfer_pubkey: signature.clone(),
                    ..Default::default()
                },
                ..Default::default()
            },
        });
        let manager = LookaheadManager::new(
            Lookahead { map },
            LookaheadProvider::None,
            UrlProvider::UrlMap(url_mapping),
        );
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers = TestBuilder {
            managers: Some(managers),
            test_service: Some(12006),
            forward_service: 12007,
        }
        .build()
        .await?;

        batch_requests(12007, 10).await?;

        let cnt_res = reqwest::get("http://localhost:12006/cnt").await.unwrap();
        assert_eq!(StatusCode::OK, cnt_res.status());
        assert_eq!(cnt_res.text().await.unwrap(), "10");
        Ok(())
    }

    #[tokio::test]
    async fn test_no_pubkey() -> Result<()> {
        let signature: BlsPublicKey = BlsPublicKey::from([42u8; BLS_PUBLIC_KEY_BYTES_LEN]);
        let map = Arc::new(DashMap::new());
        let mut provider = HashMap::new();
        provider.insert(signature, Url::from_str("http://localhost:12010/1").unwrap());
        map.insert(0, LookaheadEntry { url: "".into(), ..Default::default() });
        let manager = LookaheadManager::new(
            Lookahead { map },
            LookaheadProvider::None,
            UrlProvider::UrlMap(provider),
        );
        let mut managers = HashMap::new();
        managers.insert(1u16, manager);
        let _handlers =
            TestBuilder { managers: Some(managers), test_service: None, forward_service: 12008 }
                .build()
                .await?;
        let res = reqwest::Client::new().post("http://localhost:12008/1").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            res.text().await.unwrap(),
            format!(
                "could not find key for pubkey {}",
                BlsPublicKey::from([0u8; BLS_PUBLIC_KEY_BYTES_LEN]).to_string()
            )
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_chain_id_not_found() -> Result<()> {
        let mut managers = HashMap::new();
        managers.insert(2, LookaheadManager::default());
        let _handlers =
            TestBuilder { managers: Some(managers), test_service: None, forward_service: 12009 }
                .build()
                .await?;
        let res = reqwest::Client::new().post("http://localhost:12009/2").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(res.text().await.unwrap(), "no lookahead provider found");
        Ok(())
    }

    async fn batch_requests(port: u16, no_requests: u16) -> Result<()> {
        for _ in 0..no_requests {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_str("application/json").unwrap());
            let res = reqwest::Client::new()
                .post(format!("http://localhost:{}/1", port))
                .body("dummy plain body")
                .headers(headers)
                .headers(HeaderMap::new())
                .send()
                .await?;
            assert_eq!(res.status(), StatusCode::OK);
        }
        Ok(())
    }

    async fn handle_request(
        State(state): State<Arc<Mutex<DummySharedState>>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl IntoResponse {
        assert_eq!("dummy plain body", String::from_utf8(body.into()).unwrap());
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
        {
            let mut s = state.lock().unwrap();
            s.cnt += 1;
        }
        StatusCode::OK
    }
    async fn counter(State(state): State<Arc<Mutex<DummySharedState>>>) -> impl IntoResponse {
        let s = state.lock().unwrap();
        s.cnt.to_string().into_response()
    }
}
