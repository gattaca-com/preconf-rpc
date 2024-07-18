use std::{sync::Arc, time::Instant};

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::post,
    Router,
};
use bytes::Bytes;
use eyre::{Context, Result};
use hashbrown::HashMap;
use http::Extensions;
use reqwest::{Request, Response, StatusCode, Url};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{
    default_on_request_end, reqwest_otel_span, ReqwestOtelSpanBackend, TracingMiddleware,
};
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::Span;

pub(crate) struct SharedState {
    map_addr: HashMap<u16, Url>,
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
        reqwest_otel_span!(name = "example-request", req, time_elapsed = tracing::field::Empty)
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
    pub fn new(map_addr: HashMap<u16, Url>) -> Self {
        Self {
            map_addr,
            client: ClientBuilder::new(reqwest::Client::new())
                .with(TracingMiddleware::<TimeTrace>::new())
                .build(),
        }
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
    body: Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if let Some(address) = state.as_ref().map_addr.get(&chain_id) {
        match inner_forward_request(body, address.clone(), &state.client).await {
            Ok(res) => Ok(res),
            Err(_) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "error while forwarding request".to_string(),
            )),
        }
    } else {
        Err((StatusCode::BAD_REQUEST, format!("unknown chain_id value {}", chain_id)))
    }
}

async fn forward_request(State(_state): State<Arc<SharedState>>) -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, "missing chain_id parameter")
}

async fn inner_forward_request(
    bytes: Bytes,
    to_addr: Url,
    client: &ClientWithMiddleware,
) -> Result<impl IntoResponse> {
    let res = client.post(to_addr).body(bytes).send().await?;
    let body = res.bytes().await?;
    Ok(body)
}

#[cfg(test)]
mod test {
    use std::{
        str::FromStr,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        extract::State,
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use bytes::Bytes;
    use eyre::Result;
    use hashbrown::HashMap;
    use http::StatusCode;
    use reqwest::Url;

    use crate::forward_service::{router, SharedState};

    struct DummySharedState {
        cnt: i32,
    }

    #[tokio::test]
    async fn test_missing_chain_id() -> Result<()> {
        tokio::spawn(async move {
            let router = router(SharedState::new(Default::default()));
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
    async fn test_unknown_chain_id() -> Result<()> {
        tokio::spawn(async move {
            let router = router(SharedState::new(Default::default()));
            let listener = tokio::net::TcpListener::bind("localhost:12002").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let res = reqwest::Client::new().post("http://localhost:12002/1").send().await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.text().await.unwrap(), "unknown chain_id value 1");
        Ok(())
    }

    #[tokio::test]
    async fn test_unavailable_forwarded_service() -> Result<()> {
        tokio::spawn(async move {
            let mut map = HashMap::new();
            map.insert(1u16, Url::from_str("http://not-a-valid-url.gattaca").unwrap());
            let router = router(SharedState::new(map));
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
            let mut map = HashMap::new();
            map.insert(1u16, Url::from_str("http://localhost:12004").unwrap());
            let router = router(SharedState::new(map));
            let listener = tokio::net::TcpListener::bind("localhost:12005").await.unwrap();
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        for _ in 0..10 {
            let res = reqwest::Client::new()
                .post("http://localhost:12005/1")
                .body("dummy plain body")
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
        body: Bytes,
    ) -> impl IntoResponse {
        assert_eq!("dummy plain body", String::from_utf8(body.into()).unwrap());
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
