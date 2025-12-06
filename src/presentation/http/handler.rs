use crate::application::cdn_service::{CdnService, FetchResult};
use crate::domain::cache::CachedResponse;
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};

const HEADER_X_CACHE: &str = "X-Cache";

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn handle_request(
    State(state): State<Arc<CdnService>>,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path().to_string();
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());

    match state.fetch_content(&path, if_none_match).await {
        FetchResult::Cached(cached) => {
            info!(path = %path, "Cache hit");
            build_response(&cached, true)
        }
        FetchResult::Fresh(cached) => {
            info!(path = %path, "Cache miss");
            build_response(&cached, false)
        }
        FetchResult::NotModified => {
            info!(path = %path, "Not modified");
            StatusCode::NOT_MODIFIED.into_response()
        }
        FetchResult::NotFound(reason) => {
            warn!(path = %path, reason = %reason, "Not found");
            (StatusCode::NOT_FOUND, format!("Not found: {}", path)).into_response()
        }
    }
}

fn build_response(cached: &CachedResponse, from_cache: bool) -> Response {
    let cache_status = if from_cache { "HIT" } else { "MISS" };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &cached.content_type)
        .header(header::ETAG, format!("\"{}\"", &cached.etag))
        .header(HEADER_X_CACHE, cache_status);

    for (key, value) in &cached.headers {
        if key.to_lowercase() != "content-type" {
            response = response.header(key.as_str(), value.as_str());
        }
    }

    response
        .body(Body::from(cached.body.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
