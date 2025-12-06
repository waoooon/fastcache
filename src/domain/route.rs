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
