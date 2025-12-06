use app::application::cdn_service::CdnService;
use app::domain::cache::CdnCache;
use app::domain::origin::Origin;
use app::domain::route::{OriginEntry, Route, Router};
use app::infrastructure::origin::local::LocalOrigin;
use app::presentation::http::handler::{handle_request, AppState};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::routing::any;
use axum::Router as AxumRouter;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

fn create_test_app(root_path: &std::path::Path) -> AxumRouter {
    let cache = CdnCache::new(100, 3600);

    let origin: Arc<dyn Origin> = Arc::new(LocalOrigin::new(root_path));
    let routes = vec![Route {
        path_prefix: "/static".to_string(),
        origins: vec![OriginEntry {
            origin,
            cache_ttl: 3600,
        }],
    }];
    let router = Router::new(routes);

    let cdn_service = Arc::new(CdnService::new(router, cache));
    let app_state = AppState {
        cdn_service,
        max_body_size: 10 * 1024 * 1024,
    };

    AxumRouter::new()
        .fallback(any(handle_request))
        .with_state(app_state)
}

#[tokio::test]
async fn test_get_request_cache_miss_then_hit() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "Hello, Cache Test!").unwrap();

    let app = create_test_app(dir.path());

    // First request: MISS
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/static/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("X-Cache").unwrap(), "MISS");

    // Second request: HIT
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/static/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("X-Cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_post_to_local_origin_fails() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "Hello").unwrap();

    let app = create_test_app(dir.path());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/static/test.txt")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"key": "value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // LocalOrigin doesn't support POST, so it returns 404 (NotFound)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_etag_and_304_not_modified() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("etag-test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "ETag Test Content").unwrap();

    let app = create_test_app(dir.path());

    // First request to get ETag
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/static/etag-test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let etag = response
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Second request with If-None-Match header
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/static/etag-test.txt")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn test_file_not_found_returns_404() {
    let dir = tempdir().unwrap();
    let app = create_test_app(dir.path());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/static/nonexistent.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_head_request_works() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("head-test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "HEAD Test").unwrap();

    let app = create_test_app(dir.path());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::HEAD)
                .uri("/static/head-test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("X-Cache").is_some());
}

#[tokio::test]
async fn test_no_matching_route_returns_404() {
    let dir = tempdir().unwrap();
    let app = create_test_app(dir.path());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/unknown/path")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
