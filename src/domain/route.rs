use super::origin::Origin;
use std::sync::Arc;

pub struct OriginEntry {
    pub origin: Arc<dyn Origin>,
    pub cache_ttl: u64,
}

pub struct Route {
    pub path_prefix: String,
    pub origins: Vec<OriginEntry>,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new(routes: Vec<Route>) -> Self {
        let mut routes = routes;
        routes.sort_by(|a, b| b.path_prefix.len().cmp(&a.path_prefix.len()));
        Router { routes }
    }

    pub fn match_route<'a>(&'a self, path: &'a str) -> Option<(&'a Route, &'a str)> {
        for route in &self.routes {
            if path.starts_with(&route.path_prefix) || route.path_prefix == "/" {
                let relative_path = if route.path_prefix == "/" {
                    path
                } else {
                    path.strip_prefix(&route.path_prefix).unwrap_or(path)
                };
                return Some((route, relative_path));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::origin::{Origin, OriginRequest, OriginResponse};
    use anyhow::Result;
    use async_trait::async_trait;
    use bytes::Bytes;
    use std::collections::HashMap;

    struct MockOrigin {
        name: String,
    }

    #[async_trait]
    impl Origin for MockOrigin {
        async fn fetch(&self, _request: OriginRequest) -> Result<OriginResponse> {
            Ok(OriginResponse {
                body: Bytes::from(format!("from {}", self.name)),
                content_type: "text/plain".to_string(),
                headers: HashMap::new(),
            })
        }
    }

    fn create_route(path: &str, name: &str) -> Route {
        Route {
            path_prefix: path.to_string(),
            origins: vec![OriginEntry {
                origin: Arc::new(MockOrigin {
                    name: name.to_string(),
                }),
                cache_ttl: 3600,
            }],
        }
    }

    #[test]
    fn test_exact_path_match() {
        let routes = vec![create_route("/static", "static-origin")];
        let router = Router::new(routes);

        let result = router.match_route("/static/file.txt");
        assert!(result.is_some());
        let (route, relative) = result.unwrap();
        assert_eq!(route.path_prefix, "/static");
        assert_eq!(relative, "/file.txt");
    }

    #[test]
    fn test_longest_prefix_match() {
        let routes = vec![
            create_route("/api", "api-origin"),
            create_route("/api/v2", "api-v2-origin"),
        ];
        let router = Router::new(routes);

        // /api/v2/users should match /api/v2 (longer prefix)
        let result = router.match_route("/api/v2/users");
        assert!(result.is_some());
        let (route, relative) = result.unwrap();
        assert_eq!(route.path_prefix, "/api/v2");
        assert_eq!(relative, "/users");

        // /api/v1/users should match /api
        let result = router.match_route("/api/v1/users");
        assert!(result.is_some());
        let (route, relative) = result.unwrap();
        assert_eq!(route.path_prefix, "/api");
        assert_eq!(relative, "/v1/users");
    }

    #[test]
    fn test_root_path_match() {
        let routes = vec![
            create_route("/static", "static-origin"),
            create_route("/", "root-origin"),
        ];
        let router = Router::new(routes);

        // /static/file.txt should match /static first
        let result = router.match_route("/static/file.txt");
        assert!(result.is_some());
        let (route, _) = result.unwrap();
        assert_eq!(route.path_prefix, "/static");

        // /other/path should match / as fallback
        let result = router.match_route("/other/path");
        assert!(result.is_some());
        let (route, relative) = result.unwrap();
        assert_eq!(route.path_prefix, "/");
        assert_eq!(relative, "/other/path");
    }

    #[test]
    fn test_no_match() {
        let routes = vec![create_route("/static", "static-origin")];
        let router = Router::new(routes);

        let result = router.match_route("/api/endpoint");
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_relative_path() {
        let routes = vec![create_route("/static", "static-origin")];
        let router = Router::new(routes);

        let result = router.match_route("/static");
        assert!(result.is_some());
        let (_, relative) = result.unwrap();
        assert_eq!(relative, "");
    }

    #[test]
    fn test_routes_sorted_by_length() {
        let routes = vec![
            create_route("/a", "short"),
            create_route("/a/b/c", "long"),
            create_route("/a/b", "medium"),
        ];
        let router = Router::new(routes);

        // Verify internal sorting by checking match behavior
        let result = router.match_route("/a/b/c/d");
        assert!(result.is_some());
        let (route, _) = result.unwrap();
        assert_eq!(route.path_prefix, "/a/b/c");
    }
}
