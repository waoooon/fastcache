use crate::domain::cache::CdnCache;

pub struct CacheService {
    cache: CdnCache,
}

pub enum CacheOperationResult {
    Ok { message: String },
    Stats { entry_count: u64 },
}

impl CacheService {
    pub fn new(cache: CdnCache) -> Self {
        Self { cache }
    }

    pub fn cache(&self) -> &CdnCache {
        &self.cache
    }

    pub async fn purge(&self, path: &str) -> CacheOperationResult {
        self.cache.remove(path).await;
        CacheOperationResult::Ok {
            message: format!("Purged: {}", path),
        }
    }

    pub async fn purge_prefix(&self, prefix: &str) -> CacheOperationResult {
        self.cache.purge_by_prefix(prefix).await;
        CacheOperationResult::Ok {
            message: format!("Purged prefix: {}", prefix),
        }
    }

    pub async fn purge_all(&self) -> CacheOperationResult {
        self.cache.clear().await;
        CacheOperationResult::Ok {
            message: "All cache cleared".to_string(),
        }
    }

    pub fn stats(&self) -> CacheOperationResult {
        CacheOperationResult::Stats {
            entry_count: self.cache.entry_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::collections::HashMap;

    fn create_cached_response(body: &str) -> crate::domain::cache::CachedResponse {
        crate::domain::cache::CachedResponse {
            body: Bytes::from(body.to_string()),
            content_type: "text/plain".to_string(),
            etag: format!("etag-{}", body),
            headers: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_new_and_cache_accessor() {
        let cache = CdnCache::new(100, 3600);
        let service = CacheService::new(cache);

        // cache() should return reference to internal cache
        assert_eq!(service.cache().entry_count(), 0);
    }

    #[tokio::test]
    async fn test_purge_single_path() {
        let cache = CdnCache::new(100, 3600);
        cache
            .insert("/test".to_string(), create_cached_response("test"), None)
            .await;
        let service = CacheService::new(cache);

        assert!(service.cache().get("/test").await.is_some());

        let result = service.purge("/test").await;
        match result {
            CacheOperationResult::Ok { message } => {
                assert!(message.contains("/test"));
            }
            _ => panic!("Expected Ok result"),
        }

        assert!(service.cache().get("/test").await.is_none());
    }

    #[tokio::test]
    async fn test_purge_prefix() {
        let cache = CdnCache::new(100, 3600);
        cache
            .insert(
                "/static/a.txt".to_string(),
                create_cached_response("a"),
                None,
            )
            .await;
        cache
            .insert(
                "/static/b.txt".to_string(),
                create_cached_response("b"),
                None,
            )
            .await;
        cache
            .insert("/api/test".to_string(), create_cached_response("api"), None)
            .await;
        let service = CacheService::new(cache);

        let result = service.purge_prefix("/static/").await;
        match result {
            CacheOperationResult::Ok { message } => {
                assert!(message.contains("/static/"));
            }
            _ => panic!("Expected Ok result"),
        }

        assert!(service.cache().get("/static/a.txt").await.is_none());
        assert!(service.cache().get("/static/b.txt").await.is_none());
        assert!(service.cache().get("/api/test").await.is_some());
    }

    #[tokio::test]
    async fn test_purge_all() {
        let cache = CdnCache::new(100, 3600);
        cache
            .insert("/a".to_string(), create_cached_response("a"), None)
            .await;
        cache
            .insert("/b".to_string(), create_cached_response("b"), None)
            .await;
        let service = CacheService::new(cache);

        let result = service.purge_all().await;
        match result {
            CacheOperationResult::Ok { message } => {
                assert!(message.contains("cleared"));
            }
            _ => panic!("Expected Ok result"),
        }

        assert!(service.cache().get("/a").await.is_none());
        assert!(service.cache().get("/b").await.is_none());
    }

    #[tokio::test]
    async fn test_stats() {
        let cache = CdnCache::new(100, 3600);
        let service = CacheService::new(cache);

        let result = service.stats();
        match result {
            CacheOperationResult::Stats { entry_count } => {
                assert_eq!(entry_count, 0);
            }
            _ => panic!("Expected Stats result"),
        }
    }
}
