use bytes::Bytes;
use moka::future::Cache;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct CachedResponse {
    pub body: Bytes,
    pub content_type: String,
    pub etag: String,
    pub headers: HashMap<String, String>,
}

#[derive(Clone)]
pub struct CdnCache {
    cache: Cache<String, Arc<CachedResponse>>,
}

impl CdnCache {
    pub fn new(max_capacity: u64, default_ttl: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(Duration::from_secs(default_ttl))
            .build();

        CdnCache { cache }
    }

    pub async fn get(&self, key: &str) -> Option<Arc<CachedResponse>> {
        self.cache.get(key).await
    }

    pub async fn insert(&self, key: String, response: CachedResponse, ttl: Option<u64>) {
        if let Some(ttl_secs) = ttl {
            if ttl_secs == 0 {
                return;
            }
        }
        self.cache.insert(key, Arc::new(response)).await;
    }

    pub async fn remove(&self, key: &str) {
        self.cache.remove(key).await;
    }

    pub async fn purge_by_prefix(&self, prefix: &str) {
        let keys_to_remove: Vec<String> = self
            .cache
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| (*k).clone())
            .collect();

        for key in keys_to_remove {
            self.cache.remove(&key).await;
        }
    }

    pub async fn clear(&self) {
        self.cache.invalidate_all();
    }

    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_response(body: &str) -> CachedResponse {
        CachedResponse {
            body: Bytes::from(body.to_string()),
            content_type: "text/plain".to_string(),
            etag: format!("etag-{}", body),
            headers: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let cache = CdnCache::new(100, 3600);

        let response = create_response("hello");
        cache.insert("/test".to_string(), response.clone(), None).await;

        let result = cache.get("/test").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().body, Bytes::from("hello"));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let cache = CdnCache::new(100, 3600);

        let result = cache.get("/nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_ttl_zero_not_cached() {
        let cache = CdnCache::new(100, 3600);

        let response = create_response("should not cache");
        cache.insert("/nocache".to_string(), response, Some(0)).await;

        let result = cache.get("/nocache").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_remove() {
        let cache = CdnCache::new(100, 3600);

        let response = create_response("to be removed");
        cache.insert("/remove-me".to_string(), response, None).await;

        assert!(cache.get("/remove-me").await.is_some());

        cache.remove("/remove-me").await;

        assert!(cache.get("/remove-me").await.is_none());
    }

    #[tokio::test]
    async fn test_purge_by_prefix() {
        let cache = CdnCache::new(100, 3600);

        cache.insert("/static/a.txt".to_string(), create_response("a"), None).await;
        cache.insert("/static/b.txt".to_string(), create_response("b"), None).await;
        cache.insert("/api/endpoint".to_string(), create_response("api"), None).await;

        // Verify inserted via get (entry_count is async in moka)
        assert!(cache.get("/static/a.txt").await.is_some());
        assert!(cache.get("/static/b.txt").await.is_some());
        assert!(cache.get("/api/endpoint").await.is_some());

        cache.purge_by_prefix("/static/").await;

        assert!(cache.get("/static/a.txt").await.is_none());
        assert!(cache.get("/static/b.txt").await.is_none());
        assert!(cache.get("/api/endpoint").await.is_some());
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = CdnCache::new(100, 3600);

        cache.insert("/a".to_string(), create_response("a"), None).await;
        cache.insert("/b".to_string(), create_response("b"), None).await;

        // Verify inserted via get
        assert!(cache.get("/a").await.is_some());
        assert!(cache.get("/b").await.is_some());

        cache.clear().await;

        // Note: moka's invalidate_all is async, but get should return None
        assert!(cache.get("/a").await.is_none());
        assert!(cache.get("/b").await.is_none());
    }
}
