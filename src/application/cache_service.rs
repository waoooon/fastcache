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
