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

use crate::lookahead::LookaheadManager;

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
        .route("/:scan_id", post(scan_id_forward_request))
        .route("/", post(forward_request))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(shared_state))
}

async fn scan_id_forward_request(
    State(state): State<Arc<SharedState>>,
    Path(chain_id): Path<u16>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if let Some(manager) = state.managers.get(&chain_id) {
        if let Some(entry) = manager.get_next_elected_preconfer() {
            match inner_forward_request(&state.client, &entry.url, body, headers).await {
                Ok(res) => Ok(res),
                Err(_) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "error while forwarding request".to_string(),
                )),
            }
        } else {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("no preconfer has been elected for chain_id {}", chain_id),
            ))
        }
    } else {
        Err((StatusCode::BAD_REQUEST, format!("no lookahead provider found for id {}", chain_id)))
    }
}

async fn forward_request(State(_state): State<Arc<SharedState>>) -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, "missing chain_id parameter")
}

async fn inner_forward_request(
    client: &ClientWithMiddleware,
    to_addr: &str,
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
        sync::{Arc, Mutex},
        time::Duration,
    };

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

    use crate::{
        forward_service::{router, SharedState},
        lookahead::{Lookahead, LookaheadEntry, LookaheadManager, LookaheadProvider},
    };

    struct DummySharedState {
        cnt: i32,
    }

    #[tokio::test]
    async fn test_missing_chain_id() -> Result<()> {
        tokio::spawn(async move {
            let manager = LookaheadManager::new(
                Lookahead { map: DashMap::new().into() },
                LookaheadProvider::None,
            );
            let mut managers = HashMap::new();
            managers.insert(1u16, manager);
            let router = router(SharedState::new(managers).unwrap());
            let listener = tokio::net::TcpListener::bind("localhost:12001").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let res = reqwest::Client::new().post("http://localhost:12001").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.text().await.unwrap(), "missing chain_id parameter");
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_chain_id() -> Result<()> {
        tokio::spawn(async move {
            let manager = LookaheadManager::new(
                Lookahead { map: DashMap::new().into() },
                LookaheadProvider::None,
            );
            let mut managers = HashMap::new();
            managers.insert(1u16, manager);
            let router = router(SharedState::new(managers).unwrap());
            let listener = tokio::net::TcpListener::bind("localhost:12002").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let res = reqwest::Client::new().post("http://localhost:12002/2").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.text().await.unwrap(), "no lookahead provider found for id 2");
        Ok(())
    }

    #[tokio::test]
    async fn test_unavailable_forwarded_service() -> Result<()> {
        tokio::spawn(async move {
            let map = Arc::new(DashMap::new());
            map.insert(0, LookaheadEntry {
                url: "http://not-a-valid-url".into(),
                ..Default::default()
            });
            let manager = LookaheadManager::new(Lookahead { map }, LookaheadProvider::None);
            let mut managers = HashMap::new();
            managers.insert(1u16, manager);
            let router = router(SharedState::new(managers).unwrap());
            let listener = tokio::net::TcpListener::bind("localhost:12003").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let res = reqwest::Client::new().post("http://localhost:12003/1").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        Ok(())
    }

    #[tokio::test]
    async fn test_forward_request() -> Result<()> {
        tokio::spawn(async move {
            let dst = Arc::new(Mutex::new(DummySharedState { cnt: 0 }));
            let router: Router = Router::new()
                .route("/", post(handle_request))
                .route("/cnt", get(counter))
                .with_state(dst);
            let listener = tokio::net::TcpListener::bind("localhost:12004").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::spawn(async move {
            let map = Arc::new(DashMap::new());
            map.insert(0, LookaheadEntry {
                url: "http://localhost:12004".into(),
                ..Default::default()
            });
            let manager = LookaheadManager::new(Lookahead { map }, LookaheadProvider::None);
            let mut managers = HashMap::new();
            managers.insert(1u16, manager);
            let router = router(SharedState::new(managers).unwrap());
            let listener = tokio::net::TcpListener::bind("localhost:12005").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        for _ in 0..10 {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_str("application/json").unwrap());
            let res = reqwest::Client::new()
                .post("http://localhost:12005/1")
                .body("dummy plain body")
                .headers(headers)
                .headers(HeaderMap::new())
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }

        let cnt_res = reqwest::get("http://localhost:12004/cnt").await.unwrap();
        assert_eq!(StatusCode::OK, cnt_res.status());
        assert_eq!(cnt_res.text().await.unwrap(), "10");
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
