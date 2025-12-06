use super::{AdminRequest, AdminResponse};
use crate::application::cache_service::{CacheOperationResult, CacheService};
use crate::domain::cache::CdnCache;
use anyhow::Result;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{error, info};

pub async fn start_admin_server<P: AsRef<Path>>(socket_path: P, cache: CdnCache) -> Result<()> {
    let socket_path = socket_path.as_ref();

    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    info!("Admin socket listening on {:?}", socket_path);

    let cache_service = CacheService::new(cache);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let cache = cache_service.cache().clone();
                tokio::spawn(async move {
                    let service = CacheService::new(cache);
                    if let Err(e) = handle_connection(stream, &service).await {
                        error!("Admin connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept admin connection: {}", e);
            }
        }
    }
}

async fn handle_connection(stream: tokio::net::UnixStream, service: &CacheService) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    reader.read_line(&mut line).await?;

    let request: AdminRequest = serde_json::from_str(&line)?;
    let response = process_request(request, service).await;

    let response_json = serde_json::to_string(&response)?;
    writer.write_all(response_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    Ok(())
}

async fn process_request(request: AdminRequest, service: &CacheService) -> AdminResponse {
    let result = match request {
        AdminRequest::Purge { path } => {
            info!("Cache purged: {}", path);
            service.purge(&path).await
        }
        AdminRequest::PurgePrefix { prefix } => {
            info!("Cache purged by prefix: {}", prefix);
            service.purge_prefix(&prefix).await
        }
        AdminRequest::PurgeAll => {
            info!("All cache cleared");
            service.purge_all().await
        }
        AdminRequest::Stats => service.stats(),
    };

    match result {
        CacheOperationResult::Ok { message } => AdminResponse::Ok { message },
        CacheOperationResult::Stats { entry_count } => AdminResponse::Stats { entry_count },
    }
}
