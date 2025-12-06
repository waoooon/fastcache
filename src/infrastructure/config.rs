use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/fastcache.sock";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub origins: Vec<OriginConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default = "default_socket_path")]
    pub socket: String,
}

fn default_admin_port() -> u16 {
    9090
}

fn default_socket_path() -> String {
    DEFAULT_SOCKET_PATH.to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub max_capacity: u64,
    pub default_ttl: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u64,
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
}

fn default_requests_per_second() -> u64 {
    100
}

fn default_burst_size() -> u32 {
    50
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            requests_per_second: default_requests_per_second(),
            burst_size: default_burst_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum OriginConfig {
    #[serde(rename = "local")]
    Local {
        path: String,
        root: String,
        cache_ttl: u64,
    },
    #[serde(rename = "remote")]
    Remote {
        path: String,
        url: String,
        cache_ttl: u64,
    },
}

impl OriginConfig {
    pub fn path(&self) -> &str {
        match self {
            OriginConfig::Local { path, .. } => path,
            OriginConfig::Remote { path, .. } => path,
        }
    }

    pub fn cache_ttl(&self) -> u64 {
        match self {
            OriginConfig::Local { cache_ttl, .. } => *cache_ttl,
            OriginConfig::Remote { cache_ttl, .. } => *cache_ttl,
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                admin_port: default_admin_port(),
                socket: default_socket_path(),
            },
            cache: CacheConfig {
                max_capacity: 10000,
                default_ttl: 3600,
            },
            origins: vec![OriginConfig::Local {
                path: "/".to_string(),
                root: "./public".to_string(),
                cache_ttl: 3600,
            }],
            rate_limit: RateLimitConfig::default(),
            tls: None,
        }
    }
}
