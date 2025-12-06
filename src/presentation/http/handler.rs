use crate::application::cdn_service::{CdnService, FetchResult};
use crate::domain::cache::CachedResponse;
use crate::domain::origin::OriginResponse;
use axum::{
    body::Body,
    extract::State,
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

const HEADER_X_CACHE: &str = "X-Cache";

#[derive(Clone)]
pub struct AppState {
    pub cdn_service: Arc<CdnService>,
    pub max_body_size: usize,
}

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

pub async fn handle_request(State(state): State<AppState>, request: Request<Body>) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract headers to forward
    let mut forward_headers = HashMap::new();
    if let Some(ct) = request.headers().get(header::CONTENT_TYPE) {
        if let Ok(v) = ct.to_str() {
            forward_headers.insert("Content-Type".to_string(), v.to_string());
        }
    }

    // Extract body for non-GET/HEAD requests
    let body: Option<Bytes> = if method != Method::GET && method != Method::HEAD {
        match axum::body::to_bytes(request.into_body(), state.max_body_size).await {
            Ok(bytes) if !bytes.is_empty() => Some(bytes),
            Ok(_) => None,
            Err(_) => {
                return (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    format!(
                        "Request body exceeds maximum size of {} bytes",
                        state.max_body_size
                    ),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    match state
        .cdn_service
        .handle_request(
            method,
            &path,
            body,
            forward_headers,
            if_none_match.as_deref(),
        )
        .await
    {
        FetchResult::Cached(cached) => {
            info!(path = %path, "Cache hit");
            build_cached_response(&cached, true)
        }
        FetchResult::Fresh(cached) => {
            info!(path = %path, "Cache miss");
            build_cached_response(&cached, false)
        }
        FetchResult::NotModified => {
            info!(path = %path, "Not modified");
            StatusCode::NOT_MODIFIED.into_response()
        }
        FetchResult::NotFound(reason) => {
            warn!(path = %path, reason = %reason, "Not found");
            (StatusCode::NOT_FOUND, format!("Not found: {}", path)).into_response()
        }
        FetchResult::Forwarded(origin_response) => {
            info!(path = %path, "Forwarded to origin");
            build_forwarded_response(&origin_response)
        }
    }
}

fn build_cached_response(cached: &CachedResponse, from_cache: bool) -> Response {
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

fn build_forwarded_response(origin_response: &OriginResponse) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &origin_response.content_type);

    for (key, value) in &origin_response.headers {
        if key.to_lowercase() != "content-type" {
            response = response.header(key.as_str(), value.as_str());
        }
    }

    response
        .body(Body::from(origin_response.body.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        assert_eq!(response.status, "ok");
        assert_eq!(response.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_build_cached_response_hit() {
        let cached = CachedResponse {
            body: Bytes::from("test body"),
            content_type: "text/plain".to_string(),
            etag: "abc123".to_string(),
            headers: HashMap::new(),
        };

        let response = build_cached_response(&cached, true);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(HEADER_X_CACHE).unwrap(),
            "HIT"
        );
        assert_eq!(
            response.headers().get(header::ETAG).unwrap(),
            "\"abc123\""
        );
    }

    #[test]
    fn test_build_cached_response_miss() {
        let cached = CachedResponse {
            body: Bytes::from("test body"),
            content_type: "text/plain".to_string(),
            etag: "abc123".to_string(),
            headers: HashMap::new(),
        };

        let response = build_cached_response(&cached, false);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(HEADER_X_CACHE).unwrap(),
            "MISS"
        );
    }

    #[test]
    fn test_build_cached_response_with_extra_headers() {
        let mut headers = HashMap::new();
        headers.insert("Cache-Control".to_string(), "max-age=3600".to_string());
        headers.insert("Last-Modified".to_string(), "Wed, 21 Oct 2015".to_string());

        let cached = CachedResponse {
            body: Bytes::from("test body"),
            content_type: "text/plain".to_string(),
            etag: "abc123".to_string(),
            headers,
        };

        let response = build_cached_response(&cached, true);
        assert_eq!(
            response.headers().get("Cache-Control").unwrap(),
            "max-age=3600"
        );
        assert_eq!(
            response.headers().get("Last-Modified").unwrap(),
            "Wed, 21 Oct 2015"
        );
    }

    #[test]
    fn test_build_forwarded_response() {
        let origin_response = OriginResponse {
            body: Bytes::from("forwarded body"),
            content_type: "application/json".to_string(),
            headers: HashMap::new(),
        };

        let response = build_forwarded_response(&origin_response);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_build_forwarded_response_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        // content-type in headers should be skipped (already set explicitly)
        headers.insert("Content-Type".to_string(), "text/html".to_string());

        let origin_response = OriginResponse {
            body: Bytes::from("body"),
            content_type: "application/json".to_string(),
            headers,
        };

        let response = build_forwarded_response(&origin_response);
        assert_eq!(
            response.headers().get("X-Custom").unwrap(),
            "value"
        );
        // Should use the explicit content_type, not the one in headers
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }
}
