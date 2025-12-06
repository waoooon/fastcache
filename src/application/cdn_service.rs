use crate::domain::cache::{CachedResponse, CdnCache};
use crate::domain::origin::{OriginRequest, OriginResponse};
use crate::domain::route::Router;
use axum::http::Method;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::debug;

pub enum FetchResult {
    Cached(CachedResponse),
    Fresh(CachedResponse),
    NotModified,
    NotFound(String),
    Forwarded(OriginResponse),
}

pub struct CdnService {
    pub router: Router,
    pub cache: CdnCache,
}

impl CdnService {
    pub fn new(router: Router, cache: CdnCache) -> Self {
        Self { router, cache }
    }

    pub async fn handle_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Bytes>,
        headers: HashMap<String, String>,
        if_none_match: Option<&str>,
    ) -> FetchResult {
        match method {
            Method::GET | Method::HEAD => self.fetch_content_cached(path, if_none_match).await,
            _ => self.forward_to_origin(method, path, body, headers).await,
        }
    }

    async fn fetch_content_cached(&self, path: &str, if_none_match: Option<&str>) -> FetchResult {
        if let Some(cached) = self.cache.get(path).await {
            if etag_matches(if_none_match, &cached.etag) {
                return FetchResult::NotModified;
            }
            return FetchResult::Cached((*cached).clone());
        }

        let Some((route, relative_path)) = self.router.match_route(path) else {
            return FetchResult::NotFound(format!("No matching route for {}", path));
        };

        let relative_path = if relative_path.is_empty() {
            "/"
        } else {
            relative_path
        };

        let mut last_error = String::new();
        for (i, origin_entry) in route.origins.iter().enumerate() {
            let request = OriginRequest::get(relative_path);
            match origin_entry.origin.fetch(request).await {
                Ok(origin_response) => {
                    let etag = generate_etag(&origin_response.body);

                    let cached_response = CachedResponse {
                        body: origin_response.body,
                        content_type: origin_response.content_type,
                        etag: etag.clone(),
                        headers: origin_response.headers,
                    };

                    if origin_entry.cache_ttl > 0 {
                        self.cache
                            .insert(
                                path.to_string(),
                                cached_response.clone(),
                                Some(origin_entry.cache_ttl),
                            )
                            .await;
                    }

                    if etag_matches(if_none_match, &etag) {
                        return FetchResult::NotModified;
                    }

                    return FetchResult::Fresh(cached_response);
                }
                Err(e) => {
                    last_error = e.to_string();
                    debug!(
                        path = path,
                        origin_index = i,
                        error = %e,
                        "Origin fetch failed, trying fallback"
                    );
                }
            }
        }

        FetchResult::NotFound(last_error)
    }

    async fn forward_to_origin(
        &self,
        method: Method,
        path: &str,
        body: Option<Bytes>,
        headers: HashMap<String, String>,
    ) -> FetchResult {
        let Some((route, relative_path)) = self.router.match_route(path) else {
            return FetchResult::NotFound(format!("No matching route for {}", path));
        };

        let relative_path = if relative_path.is_empty() {
            "/"
        } else {
            relative_path
        };

        let mut last_error = String::new();
        for (i, origin_entry) in route.origins.iter().enumerate() {
            let request = OriginRequest {
                path: relative_path.to_string(),
                method: method.clone(),
                body: body.clone(),
                headers: headers.clone(),
            };

            match origin_entry.origin.fetch(request).await {
                Ok(origin_response) => {
                    return FetchResult::Forwarded(origin_response);
                }
                Err(e) => {
                    last_error = e.to_string();
                    debug!(
                        path = path,
                        origin_index = i,
                        error = %e,
                        "Origin forward failed, trying fallback"
                    );
                }
            }
        }

        FetchResult::NotFound(last_error)
    }
}

fn etag_matches(client_etag: Option<&str>, server_etag: &str) -> bool {
    client_etag
        .map(|e| e.trim_matches('"') == server_etag.trim_matches('"'))
        .unwrap_or(false)
}

fn generate_etag(body: &Bytes) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::origin::{Origin, OriginRequest, OriginResponse};
    use crate::domain::route::{OriginEntry, Route, Router};
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct FailingOrigin {
        name: String,
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Origin for FailingOrigin {
        async fn fetch(&self, _request: OriginRequest) -> Result<OriginResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Err(anyhow!("{} failed", self.name))
        }
    }

    struct SuccessOrigin {
        name: String,
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Origin for SuccessOrigin {
        async fn fetch(&self, _request: OriginRequest) -> Result<OriginResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(OriginResponse {
                body: Bytes::from(format!("from {}", self.name)),
                content_type: "text/plain".to_string(),
                headers: HashMap::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_fallback_order_first_succeeds() {
        let first_count = Arc::new(AtomicUsize::new(0));
        let second_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/static".to_string(),
            origins: vec![
                OriginEntry {
                    origin: Arc::new(SuccessOrigin {
                        name: "first".to_string(),
                        call_count: first_count.clone(),
                    }),
                    cache_ttl: 0,
                },
                OriginEntry {
                    origin: Arc::new(SuccessOrigin {
                        name: "second".to_string(),
                        call_count: second_count.clone(),
                    }),
                    cache_ttl: 0,
                },
            ],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        let result = service
            .handle_request(Method::GET, "/static/test.txt", None, HashMap::new(), None)
            .await;

        // First origin should be called
        assert_eq!(first_count.load(Ordering::SeqCst), 1);
        // Second origin should NOT be called (first succeeded)
        assert_eq!(second_count.load(Ordering::SeqCst), 0);

        match result {
            FetchResult::Fresh(response) => {
                assert_eq!(response.body, Bytes::from("from first"));
            }
            _ => panic!("Expected Fresh result"),
        }
    }

    #[tokio::test]
    async fn test_fallback_order_first_fails() {
        let first_count = Arc::new(AtomicUsize::new(0));
        let second_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/static".to_string(),
            origins: vec![
                OriginEntry {
                    origin: Arc::new(FailingOrigin {
                        name: "first".to_string(),
                        call_count: first_count.clone(),
                    }),
                    cache_ttl: 0,
                },
                OriginEntry {
                    origin: Arc::new(SuccessOrigin {
                        name: "second".to_string(),
                        call_count: second_count.clone(),
                    }),
                    cache_ttl: 0,
                },
            ],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        let result = service
            .handle_request(Method::GET, "/static/test.txt", None, HashMap::new(), None)
            .await;

        // First origin should be called and fail
        assert_eq!(first_count.load(Ordering::SeqCst), 1);
        // Second origin should be called as fallback
        assert_eq!(second_count.load(Ordering::SeqCst), 1);

        match result {
            FetchResult::Fresh(response) => {
                assert_eq!(response.body, Bytes::from("from second"));
            }
            _ => panic!("Expected Fresh result"),
        }
    }

    #[tokio::test]
    async fn test_fallback_all_fail() {
        let first_count = Arc::new(AtomicUsize::new(0));
        let second_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/static".to_string(),
            origins: vec![
                OriginEntry {
                    origin: Arc::new(FailingOrigin {
                        name: "first".to_string(),
                        call_count: first_count.clone(),
                    }),
                    cache_ttl: 0,
                },
                OriginEntry {
                    origin: Arc::new(FailingOrigin {
                        name: "second".to_string(),
                        call_count: second_count.clone(),
                    }),
                    cache_ttl: 0,
                },
            ],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        let result = service
            .handle_request(Method::GET, "/static/test.txt", None, HashMap::new(), None)
            .await;

        // Both origins should be called
        assert_eq!(first_count.load(Ordering::SeqCst), 1);
        assert_eq!(second_count.load(Ordering::SeqCst), 1);

        match result {
            FetchResult::NotFound(error) => {
                assert!(error.contains("second failed"));
            }
            _ => panic!("Expected NotFound result"),
        }
    }

    #[tokio::test]
    async fn test_post_request_forwarded_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/api".to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(SuccessOrigin {
                    name: "api".to_string(),
                    call_count: call_count.clone(),
                }),
                cache_ttl: 3600, // Would cache if GET
            }],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        // POST request
        let result = service
            .handle_request(
                Method::POST,
                "/api/data",
                Some(Bytes::from("request body")),
                HashMap::new(),
                None,
            )
            .await;

        // Origin should be called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Should return Forwarded (not Fresh/Cached)
        match result {
            FetchResult::Forwarded(response) => {
                assert_eq!(response.body, Bytes::from("from api"));
            }
            _ => panic!("Expected Forwarded result"),
        }

        // Cache should be empty (POST not cached)
        assert!(service.cache.get("/api/data").await.is_none());
    }

    #[tokio::test]
    async fn test_put_request_forwarded_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/api".to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(SuccessOrigin {
                    name: "api".to_string(),
                    call_count: call_count.clone(),
                }),
                cache_ttl: 3600,
            }],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        // PUT request
        let result = service
            .handle_request(Method::PUT, "/api/resource", None, HashMap::new(), None)
            .await;

        // Origin should be called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Should return Forwarded
        assert!(matches!(result, FetchResult::Forwarded(_)));

        // Cache should be empty
        assert!(service.cache.get("/api/resource").await.is_none());
    }

    #[tokio::test]
    async fn test_delete_request_forwarded_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/api".to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(SuccessOrigin {
                    name: "api".to_string(),
                    call_count: call_count.clone(),
                }),
                cache_ttl: 3600,
            }],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        // DELETE request
        let result = service
            .handle_request(Method::DELETE, "/api/resource/123", None, HashMap::new(), None)
            .await;

        // Origin should be called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Should return Forwarded
        assert!(matches!(result, FetchResult::Forwarded(_)));

        // Cache should be empty
        assert!(service.cache.get("/api/resource/123").await.is_none());
    }

    #[tokio::test]
    async fn test_patch_request_forwarded_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/api".to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(SuccessOrigin {
                    name: "api".to_string(),
                    call_count: call_count.clone(),
                }),
                cache_ttl: 3600,
            }],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        // PATCH request
        let result = service
            .handle_request(
                Method::PATCH,
                "/api/resource",
                Some(Bytes::from("partial update")),
                HashMap::new(),
                None,
            )
            .await;

        // Origin should be called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Should return Forwarded
        assert!(matches!(result, FetchResult::Forwarded(_)));

        // Cache should be empty
        assert!(service.cache.get("/api/resource").await.is_none());
    }

    #[tokio::test]
    async fn test_get_cached_but_post_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));

        let routes = vec![Route {
            path_prefix: "/api".to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(SuccessOrigin {
                    name: "api".to_string(),
                    call_count: call_count.clone(),
                }),
                cache_ttl: 3600,
            }],
        }];

        let router = Router::new(routes);
        let cache = CdnCache::new(100, 3600);
        let service = CdnService::new(router, cache);

        // First: GET request - should be cached
        let result = service
            .handle_request(Method::GET, "/api/data", None, HashMap::new(), None)
            .await;
        assert!(matches!(result, FetchResult::Fresh(_)));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Verify GET result is cached
        assert!(service.cache.get("/api/data").await.is_some());

        // Second: GET request - should hit cache
        let result = service
            .handle_request(Method::GET, "/api/data", None, HashMap::new(), None)
            .await;
        assert!(matches!(result, FetchResult::Cached(_)));
        // Origin not called again
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Third: POST request - should forward, not use cache
        let result = service
            .handle_request(
                Method::POST,
                "/api/data",
                Some(Bytes::from("body")),
                HashMap::new(),
                None,
            )
            .await;
        assert!(matches!(result, FetchResult::Forwarded(_)));
        // Origin called for POST
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
