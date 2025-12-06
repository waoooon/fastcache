use crate::domain::origin::{Origin, OriginResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
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
    async fn fetch(&self, path: &str) -> Result<OriginResponse> {
        let clean_path = path.trim_start_matches('/');
        let file_path = self.root.join(clean_path);

        let canonical = file_path
            .canonicalize()
            .map_err(|_| anyhow!("File not found: {}", path))?;

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
