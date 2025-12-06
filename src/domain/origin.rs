use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

pub struct OriginResponse {
    pub body: Bytes,
    pub content_type: String,
    pub headers: HashMap<String, String>,
}

#[async_trait]
pub trait Origin: Send + Sync {
    async fn fetch(&self, path: &str) -> Result<OriginResponse>;
}
