use crate::domain::origin::{Origin, OriginRequest, OriginResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::http::Method;
use bytes::Bytes;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct LocalOrigin {
    root: PathBuf,
}

impl LocalOrigin {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        LocalOrigin {
            root: root.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl Origin for LocalOrigin {
    async fn fetch(&self, request: OriginRequest) -> Result<OriginResponse> {
        if request.method != Method::GET && request.method != Method::HEAD {
            return Err(anyhow!(
                "Method {} not supported for local origin",
                request.method
            ));
        }

        let clean_path = request.path.trim_start_matches('/');
        let file_path = self.root.join(clean_path);

        let canonical = file_path
            .canonicalize()
            .map_err(|_| anyhow!("File not found: {}", request.path))?;

        let root_canonical = self
            .root
            .canonicalize()
            .map_err(|_| anyhow!("Root directory not found"))?;

        if !canonical.starts_with(&root_canonical) {
            return Err(anyhow!("Path traversal attempt detected"));
        }

        let metadata = fs::metadata(&canonical).await?;
        if metadata.is_dir() {
            let index_path = canonical.join("index.html");
            if index_path.exists() {
                return self.read_file(&index_path).await;
            }
            return Err(anyhow!("Directory listing not allowed"));
        }

        self.read_file(&canonical).await
    }
}

impl LocalOrigin {
    async fn read_file(&self, path: &Path) -> Result<OriginResponse> {
        let body = fs::read(path).await?;
        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let mut headers = HashMap::new();
        if let Ok(metadata) = fs::metadata(path).await {
            if let Ok(modified) = metadata.modified() {
                let datetime: chrono::DateTime<chrono::Utc> = modified.into();
                headers.insert(
                    "Last-Modified".to_string(),
                    datetime.format("%a, %d %b %Y %H:%M:%S GMT").to_string(),
                );
            }
        }

        Ok(OriginResponse {
            body: Bytes::from(body),
            content_type,
            headers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_get_request_succeeds() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello, World!").unwrap();

        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest::get("/test.txt");
        let response = origin.fetch(request).await.unwrap();

        assert_eq!(response.body, Bytes::from("Hello, World!\n"));
        assert_eq!(response.content_type, "text/plain");
    }

    #[tokio::test]
    async fn test_head_request_succeeds() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello").unwrap();

        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest {
            path: "/test.txt".to_string(),
            method: Method::HEAD,
            body: None,
            headers: HashMap::new(),
        };
        let response = origin.fetch(request).await.unwrap();

        assert!(!response.body.is_empty());
    }

    #[tokio::test]
    async fn test_post_request_fails() {
        let dir = tempdir().unwrap();
        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest {
            path: "/test.txt".to_string(),
            method: Method::POST,
            body: Some(Bytes::from("data")),
            headers: HashMap::new(),
        };
        let result = origin.fetch(request).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not supported for local origin"));
    }

    #[tokio::test]
    async fn test_put_request_fails() {
        let dir = tempdir().unwrap();
        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest {
            path: "/test.txt".to_string(),
            method: Method::PUT,
            body: None,
            headers: HashMap::new(),
        };
        let result = origin.fetch(request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let dir = tempdir().unwrap();
        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest::get("/nonexistent.txt");
        let result = origin.fetch(request).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_path_traversal_blocked() {
        let dir = tempdir().unwrap();
        let origin = LocalOrigin::new(dir.path());
        let request = OriginRequest::get("/../../../etc/passwd");
        let result = origin.fetch(request).await;

        assert!(result.is_err());
    }
}
