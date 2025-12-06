use crate::domain::origin::{Origin, OriginResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use reqwest::Client;
use std::collections::HashMap;

pub struct RemoteOrigin {
    base_url: String,
    client: Client,
}

impl RemoteOrigin {
    pub fn new(base_url: &str) -> Result<Self> {
        let client = Client::builder().gzip(true).brotli(true).build()?;

        Ok(RemoteOrigin {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }
}

fn extract_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(String::from)
}

#[async_trait]
impl Origin for RemoteOrigin {
    async fn fetch(&self, path: &str) -> Result<OriginResponse> {
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch from origin: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Origin returned error: {} for {}",
                response.status(),
                url
            ));
        }

        let mut headers = HashMap::new();

        if let Some(v) = extract_header(response.headers(), "content-type") {
            headers.insert("Content-Type".to_string(), v);
        }
        if let Some(v) = extract_header(response.headers(), "last-modified") {
            headers.insert("Last-Modified".to_string(), v);
        }
        if let Some(v) = extract_header(response.headers(), "cache-control") {
            headers.insert("Cache-Control".to_string(), v);
        }

        let content_type = headers
            .get("Content-Type")
            .cloned()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let body = response.bytes().await?;

        Ok(OriginResponse {
            body,
            content_type,
            headers,
        })
    }
}
