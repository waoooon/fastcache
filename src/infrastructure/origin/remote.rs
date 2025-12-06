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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_new_creates_origin() {
        let origin = RemoteOrigin::new("http://example.com");
        assert!(origin.is_ok());
    }

    #[tokio::test]
    async fn test_new_trims_trailing_slash() {
        let origin = RemoteOrigin::new("http://example.com/").unwrap();
        assert_eq!(origin.base_url, "http://example.com");
    }

    #[tokio::test]
    async fn test_get_request_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("hello world")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest::get("/test.txt");
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, bytes::Bytes::from("hello world"));
        assert_eq!(response.content_type, "text/plain");
    }

    #[tokio::test]
    async fn test_post_request_with_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/data"))
            .and(body_string("request body"))
            .respond_with(ResponseTemplate::new(200).set_body_string("response body"))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest {
            path: "/api/data".to_string(),
            method: Method::POST,
            body: Some(bytes::Bytes::from("request body")),
            headers: HashMap::new(),
        };
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, bytes::Bytes::from("response body"));
    }

    #[tokio::test]
    async fn test_put_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/api/resource"))
            .respond_with(ResponseTemplate::new(200).set_body_string("updated"))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest {
            path: "/api/resource".to_string(),
            method: Method::PUT,
            body: None,
            headers: HashMap::new(),
        };
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, bytes::Bytes::from("updated"));
    }

    #[tokio::test]
    async fn test_delete_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/resource/123"))
            .respond_with(ResponseTemplate::new(200).set_body_string("deleted"))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest {
            path: "/api/resource/123".to_string(),
            method: Method::DELETE,
            body: None,
            headers: HashMap::new(),
        };
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, bytes::Bytes::from("deleted"));
    }

    #[tokio::test]
    async fn test_request_with_custom_headers() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/data"))
            .and(header("X-Custom-Header", "custom-value"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());

        let request = OriginRequest {
            path: "/api/data".to_string(),
            method: Method::GET,
            body: None,
            headers,
        };
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, bytes::Bytes::from("ok"));
    }

    #[tokio::test]
    async fn test_error_on_non_success_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/notfound"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest::get("/notfound");
        let result = origin.fetch(request).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("404"));
    }

    #[tokio::test]
    async fn test_error_on_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest::get("/error");
        let result = origin.fetch(request).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("500"));
    }

    #[tokio::test]
    async fn test_extracts_response_headers() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/with-headers"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("content")
                    .insert_header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")
                    .insert_header("cache-control", "max-age=3600"),
            )
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest::get("/with-headers");
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(
            response.headers.get("Last-Modified"),
            Some(&"Wed, 21 Oct 2015 07:28:00 GMT".to_string())
        );
        assert_eq!(
            response.headers.get("Cache-Control"),
            Some(&"max-age=3600".to_string())
        );
    }

    #[tokio::test]
    async fn test_content_type_from_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("hello")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let origin = RemoteOrigin::new(&mock_server.uri()).unwrap();
        let request = OriginRequest::get("/test.txt");
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.content_type, "text/plain");
    }
}
