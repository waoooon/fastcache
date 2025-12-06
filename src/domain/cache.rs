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
