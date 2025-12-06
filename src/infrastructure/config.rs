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
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
}

fn default_max_body_size() -> usize {
    10 * 1024 * 1024 // 10MB
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
                max_body_size: default_max_body_size(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.admin_port, 9090);
        assert_eq!(config.server.socket, DEFAULT_SOCKET_PATH);
        assert_eq!(config.server.max_body_size, 10 * 1024 * 1024);
        assert_eq!(config.cache.max_capacity, 10000);
        assert_eq!(config.cache.default_ttl, 3600);
        assert_eq!(config.rate_limit.requests_per_second, 100);
        assert_eq!(config.rate_limit.burst_size, 50);
        assert!(config.tls.is_none());
    }

    #[test]
    fn test_load_minimal_config() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 9000
cache:
  max_capacity: 5000
  default_ttl: 1800
origins:
  - type: local
    path: "/static"
    root: "./public"
    cache_ttl: 3600
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let config = Config::load(file.path()).unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.admin_port, 9090); // default
        assert_eq!(config.cache.max_capacity, 5000);
        assert_eq!(config.cache.default_ttl, 1800);
        assert_eq!(config.origins.len(), 1);
    }

    #[test]
    fn test_load_full_config() {
        let yaml = r#"
server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 9999
  socket: "/tmp/test.sock"
  max_body_size: 1048576
cache:
  max_capacity: 10000
  default_ttl: 3600
rate_limit:
  requests_per_second: 200
  burst_size: 100
tls:
  cert_path: "/path/to/cert.pem"
  key_path: "/path/to/key.pem"
origins:
  - type: local
    path: "/static"
    root: "./public"
    cache_ttl: 86400
  - type: remote
    path: "/api"
    url: "http://backend:3000"
    cache_ttl: 0
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let config = Config::load(file.path()).unwrap();

        assert_eq!(config.server.admin_port, 9999);
        assert_eq!(config.server.socket, "/tmp/test.sock");
        assert_eq!(config.server.max_body_size, 1048576);
        assert_eq!(config.rate_limit.requests_per_second, 200);
        assert_eq!(config.rate_limit.burst_size, 100);
        assert!(config.tls.is_some());
        assert_eq!(config.tls.as_ref().unwrap().cert_path, "/path/to/cert.pem");
        assert_eq!(config.origins.len(), 2);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = Config::load("/nonexistent/path/config.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_yaml() {
        let yaml = "invalid: yaml: content: [";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let result = Config::load(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_origin_config_path() {
        let local = OriginConfig::Local {
            path: "/static".to_string(),
            root: "./public".to_string(),
            cache_ttl: 3600,
        };
        assert_eq!(local.path(), "/static");

        let remote = OriginConfig::Remote {
            path: "/api".to_string(),
            url: "http://backend:3000".to_string(),
            cache_ttl: 0,
        };
        assert_eq!(remote.path(), "/api");
    }

    #[test]
    fn test_origin_config_cache_ttl() {
        let local = OriginConfig::Local {
            path: "/static".to_string(),
            root: "./public".to_string(),
            cache_ttl: 86400,
        };
        assert_eq!(local.cache_ttl(), 86400);

        let remote = OriginConfig::Remote {
            path: "/api".to_string(),
            url: "http://backend:3000".to_string(),
            cache_ttl: 0,
        };
        assert_eq!(remote.cache_ttl(), 0);
    }

    #[test]
    fn test_rate_limit_default() {
        let rate_limit = RateLimitConfig::default();
        assert_eq!(rate_limit.requests_per_second, 100);
        assert_eq!(rate_limit.burst_size, 50);
    }
}
