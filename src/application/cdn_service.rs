use crate::domain::cache::{CachedResponse, CdnCache};
use crate::domain::route::Router;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tracing::debug;

pub enum FetchResult {
    Cached(CachedResponse),
    Fresh(CachedResponse),
    NotModified,
    NotFound(String),
}

pub struct CdnService {
    pub router: Router,
    pub cache: CdnCache,
}

impl CdnService {
    pub fn new(router: Router, cache: CdnCache) -> Self {
        Self { router, cache }
    }

    pub async fn fetch_content(&self, path: &str, if_none_match: Option<&str>) -> FetchResult {
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

        // Try each origin in order, fallback to next on failure
        let mut last_error = String::new();
        for (i, origin_entry) in route.origins.iter().enumerate() {
            match origin_entry.origin.fetch(relative_path).await {
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
