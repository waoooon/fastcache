# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# Build
cargo build

# Run server
cargo run -- serve
cargo run -- serve --config /path/to/config.yaml

# Cache management (requires running server)
cargo run -- purge /path/to/file      # Purge specific path
cargo run -- purge --prefix /static/  # Purge by prefix
cargo run -- purge --all              # Purge all
cargo run -- stats                    # Show cache stats

# Docker
make docker-build
make docker-up
make docker-down
```

## Architecture

Fastcache is a caching proxy server written in Rust that supports both local file serving and remote origin proxying. The codebase follows a **layered architecture** with 4 layers.

### Directory Structure

```
src/
├── main.rs                           # Entry point
├── presentation/                     # Presentation layer (external I/F)
│   ├── cli.rs                        # CLI argument parsing
│   ├── http/
│   │   ├── server.rs                 # HTTP server startup
│   │   ├── handler.rs                # Request handler
│   │   └── middleware.rs             # Access logging middleware
│   └── admin/
│       ├── socket_server.rs          # Unix socket server
│       └── socket_client.rs          # Unix socket client
├── application/                      # Application layer (use cases)
│   ├── cdn_service.rs                # CDN delivery logic
│   └── cache_service.rs              # Cache operation logic
├── domain/                           # Domain layer (business rules)
│   ├── cache.rs                      # Cache entity
│   ├── route.rs                      # Routing rules
│   └── origin.rs                     # Origin trait definition
└── infrastructure/                   # Infrastructure layer (external systems)
    ├── config.rs                     # Config file loading
    └── origin/
        ├── local.rs                  # Local filesystem
        └── remote.rs                 # Remote HTTP
```

### Layer Responsibilities

| Layer | Responsibility |
|-------|----------------|
| **Presentation** | External interfaces (CLI, HTTP handlers, Unix socket) |
| **Application** | Use case orchestration (CDN delivery, cache operations) |
| **Domain** | Business rules (cache, routing, Origin trait) |
| **Infrastructure** | External system connections (config, filesystem, HTTP) |

### Dependency Direction

```
Presentation → Application → Domain ← Infrastructure
```

### Request Flow

```
Request → Rate Limit → Router (path matching) → Origin (local/remote) → Cache → Response
                                                                          ↑
                                                                 (moka TTL cache)
```

### Key Components

- **CdnService (`application/cdn_service.rs`)**: Orchestrates cache check → origin fetch → cache store
- **CacheService (`application/cache_service.rs`)**: Handles purge/stats operations
- **Router (`domain/route.rs`)**: Matches request paths to origins using longest-prefix matching
- **Origin trait (`domain/origin.rs`)**: Common interface for fetching content
  - `LocalOrigin`: Serves files from filesystem with path traversal protection
  - `RemoteOrigin`: Proxies to upstream HTTP server via reqwest
- **CdnCache (`domain/cache.rs`)**: moka-based TTL cache wrapping `CachedResponse`

### Configuration

YAML config with tagged enum for origin types:
```yaml
server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 9090
  socket: "/tmp/fastcache.sock"

cache:
  max_capacity: 10000
  default_ttl: 3600

rate_limit:
  requests_per_second: 100
  burst_size: 50

# Optional: TLS configuration
tls:
  cert_path: "/path/to/cert.pem"
  key_path: "/path/to/key.pem"

origins:
  - path: "/static"
    type: local
    root: "./public"
    cache_ttl: 86400
  - path: "/api"
    type: remote
    url: "http://backend:3000"
    cache_ttl: 0
```

### Production Features

| Feature | Description |
|---------|-------------|
| **Health Check** | `GET /health` on admin port (default: 9090) returns JSON with status and version |
| **Rate Limiting** | IP-based rate limiting using token bucket algorithm |
| **Access Logging** | Structured logs with client IP, method, path, status, duration |
| **TLS/HTTPS** | Native TLS support via rustls (optional) |

### Origin Fallback

Multiple origins can be configured for the same path prefix. Origins are tried in config order; if one fails, the next is tried.

```yaml
origins:
  - path: "/static"
    type: local
    root: "./public"
    cache_ttl: 86400
  - path: "/static"        # Same path = fallback
    type: remote
    url: "https://cdn.example.com"
    cache_ttl: 3600
```

### Design Decisions

- Health check runs on separate admin port to avoid conflicts with CDN paths
- Admin operations use Unix socket (`/tmp/fastcache.sock`) instead of HTTP endpoint to avoid exposing management API
- Same path origins are grouped and tried in order (fallback chain)
- Cache key is the full request path
- ETag generated from SHA256 hash of response body
- `X-Cache: HIT/MISS` header indicates cache status
- Rate limiting returns HTTP 429 when exceeded
