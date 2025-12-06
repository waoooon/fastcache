use crate::domain::origin::{Origin, OriginRequest, OriginResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::http::Method;
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
    async fn fetch(&self, request: OriginRequest) -> Result<OriginResponse> {
        let url = format!("{}{}", self.base_url, request.path);

        let mut req_builder = match request.method {
            Method::GET => self.client.get(&url),
            Method::POST => self.client.post(&url),
            Method::PUT => self.client.put(&url),
            Method::PATCH => self.client.patch(&url),
            Method::DELETE => self.client.delete(&url),
            Method::HEAD => self.client.head(&url),
            _ => return Err(anyhow!("Unsupported method: {}", request.method)),
        };

        // Forward request headers
        for (key, value) in &request.headers {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }

        // Forward request body
        if let Some(body) = request.body {
            req_builder = req_builder.body(body);
        }

        let response = req_builder
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
