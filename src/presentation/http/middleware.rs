use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, Response},
    middleware::Next,
};
use std::net::SocketAddr;
use std::time::Instant;
use tracing::info;

pub async fn access_log(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status().as_u16();

    info!(
        target: "access_log",
        client_ip = %addr.ip(),
        method = %method,
        path = %uri.path(),
        query = uri.query().unwrap_or("-"),
        status = status,
        duration_ms = duration.as_millis() as u64,
        user_agent = %user_agent,
        "request"
    );

    response
}
