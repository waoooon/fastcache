use anyhow::Result;
use async_trait::async_trait;
use axum::http::Method;
use bytes::Bytes;
use std::collections::HashMap;

pub struct OriginRequest {
    pub path: String,
    pub method: Method,
    pub body: Option<Bytes>,
    pub headers: HashMap<String, String>,
}

impl OriginRequest {
    pub fn get(path: &str) -> Self {
        Self {
            path: path.to_string(),
            method: Method::GET,
            body: None,
            headers: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct OriginResponse {
    pub body: Bytes,
    pub content_type: String,
    pub headers: HashMap<String, String>,
}

#[async_trait]
pub trait Origin: Send + Sync {
    async fn fetch(&self, request: OriginRequest) -> Result<OriginResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_request_get() {
        let request = OriginRequest::get("/test/path");

        assert_eq!(request.path, "/test/path");
        assert_eq!(request.method, Method::GET);
        assert!(request.body.is_none());
        assert!(request.headers.is_empty());
    }

    #[test]
    fn test_origin_request_get_empty_path() {
        let request = OriginRequest::get("");

        assert_eq!(request.path, "");
        assert_eq!(request.method, Method::GET);
    }
}
