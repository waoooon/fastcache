use crate::application::cdn_service::CdnService;
use crate::domain::cache::CdnCache;
use crate::domain::origin::Origin;
use crate::domain::route::{OriginEntry, Route, Router};
use crate::infrastructure::config::{Config, OriginConfig, TlsConfig};
use crate::infrastructure::origin::local::LocalOrigin;
use crate::infrastructure::origin::remote::RemoteOrigin;
use crate::presentation::admin::socket_server;
use crate::presentation::http::handler::{handle_request, health_check, AppState};
use crate::presentation::http::middleware::access_log;
use axum::{middleware, routing::any, routing::get, Router as AxumRouter};
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::compression::CompressionLayer;
use tracing::info;

async fn start_admin_http_server(addr: SocketAddr) -> anyhow::Result<()> {
    let admin_app = AxumRouter::new().route("/health", get(health_check));

    info!("Admin server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, admin_app).await?;
    Ok(())
}

async fn start_cdn_server(
    addr: SocketAddr,
    tls_config: Option<&TlsConfig>,
    cdn_app: AxumRouter<()>,
) -> anyhow::Result<()> {
    match tls_config {
        Some(tls) => {
            info!("CDN server listening on https://{}", addr);
            let rustls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
                    .await?;
            axum_server::bind_rustls(addr, rustls_config)
                .serve(cdn_app.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        }
        None => {
            info!("CDN server listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(
                listener,
                cdn_app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
    }
    Ok(())
}

async fn run_servers<F: Future<Output = anyhow::Result<()>>>(
    cdn_server: F,
    admin_addr: SocketAddr,
    socket_path: &str,
    cache: CdnCache,
) -> anyhow::Result<()> {
    tokio::select! {
        result = cdn_server => result?,
        result = start_admin_http_server(admin_addr) => result?,
        result = socket_server::start_admin_server(socket_path, cache) => result?,
    }
    Ok(())
}

pub async fn run<P: AsRef<Path>>(config_path: P) -> anyhow::Result<()> {
    let config = Config::load(config_path.as_ref()).unwrap_or_else(|e| {
        info!(
            "Failed to load config from {:?}: {}, using defaults",
            config_path.as_ref(),
            e
        );
        Config::default()
    });

    let cache = CdnCache::new(config.cache.max_capacity, config.cache.default_ttl);
    let router = build_router(&config.origins);

    let cdn_service = Arc::new(CdnService::new(router, cache.clone()));

    let app_state = AppState {
        cdn_service,
        max_body_size: config.server.max_body_size,
    };

    // Rate limiter configuration
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(config.rate_limit.requests_per_second)
            .burst_size(config.rate_limit.burst_size)
            .finish()
            .expect("Failed to create rate limiter config"),
    );

    let cdn_app = AxumRouter::new()
        .fallback(any(handle_request))
        .layer(middleware::from_fn(access_log))
        .layer(GovernorLayer {
            config: governor_conf,
        })
        .layer(CompressionLayer::new())
        .with_state(app_state);

    let cdn_addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    let admin_addr: SocketAddr =
        format!("{}:{}", config.server.host, config.server.admin_port).parse()?;
    let socket_path = config.server.socket.clone();

    run_servers(
        start_cdn_server(cdn_addr, config.tls.as_ref(), cdn_app),
        admin_addr,
        &socket_path,
        cache,
    )
    .await
}

fn build_router(origins: &[OriginConfig]) -> Router {
    // Group origins by path prefix, preserving order for fallback
    let mut path_origins: HashMap<String, Vec<OriginEntry>> = HashMap::new();
    let mut path_order: Vec<String> = Vec::new();

    for config in origins {
        let path = config.path().to_string();
        let origin: Arc<dyn Origin> = match config {
            OriginConfig::Local { root, .. } => Arc::new(LocalOrigin::new(root)),
            OriginConfig::Remote { url, .. } => {
                Arc::new(RemoteOrigin::new(url).expect("Failed to create HTTP client"))
            }
        };

        let entry = OriginEntry {
            origin,
            cache_ttl: config.cache_ttl(),
        };

        if !path_origins.contains_key(&path) {
            path_order.push(path.clone());
        }
        path_origins.entry(path).or_default().push(entry);
    }

    let routes: Vec<Route> = path_order
        .into_iter()
        .map(|path| Route {
            path_prefix: path.clone(),
            origins: path_origins.remove(&path).unwrap_or_default(),
        })
        .collect();

    Router::new(routes)
}
