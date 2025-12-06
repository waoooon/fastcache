use super::{AdminRequest, AdminResponse};
use anyhow::{anyhow, Result};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn send_request<P: AsRef<Path>>(
    socket_path: P,
    request: AdminRequest,
) -> Result<AdminResponse> {
    let stream = UnixStream::connect(socket_path.as_ref())
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to connect to admin socket: {}. Is the server running?",
                e
            )
        })?;

    let (reader, mut writer) = stream.into_split();

    let request_json = serde_json::to_string(&request)?;
    writer.write_all(request_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: AdminResponse = serde_json::from_str(&line)?;
    Ok(response)
}

pub async fn purge_path<P: AsRef<Path>>(socket_path: P, path: &str) -> Result<AdminResponse> {
    send_request(
        socket_path,
        AdminRequest::Purge {
            path: path.to_string(),
        },
    )
    .await
}

pub async fn purge_prefix<P: AsRef<Path>>(socket_path: P, prefix: &str) -> Result<AdminResponse> {
    send_request(
        socket_path,
        AdminRequest::PurgePrefix {
            prefix: prefix.to_string(),
        },
    )
    .await
}

pub async fn purge_all<P: AsRef<Path>>(socket_path: P) -> Result<AdminResponse> {
    send_request(socket_path, AdminRequest::PurgeAll).await
}

pub async fn stats<P: AsRef<Path>>(socket_path: P) -> Result<AdminResponse> {
    send_request(socket_path, AdminRequest::Stats).await
}
