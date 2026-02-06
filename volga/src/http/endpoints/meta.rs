//! Utilities for routes metadata

use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use crate::http::Method;

/// Represents a route metadata
#[derive(Debug, PartialEq)]
pub(crate) struct RouteInfo {
    pub(super) method: Method,
    pub(super) path: String,
}

/// Consumes all available routes metadata
pub(crate) struct RoutesInfo(pub(super) Vec<RouteInfo>);

impl Deref for RoutesInfo {
    type Target = Vec<RouteInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RoutesInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for RoutesInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        writeln!(f, "Available routes:")?;
        writeln!(f)?;
        for route in &self.0 {
            writeln!(f, "{route}")?;
        }
        Ok(())
    }
}

impl Display for RouteInfo {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let colored_method = match &self.method {
            &Method::GET => format!("\x1b[1;34m{:<8}\x1b[0m", "GET"),
            &Method::POST => format!("\x1b[1;32m{:<8}\x1b[0m", "POST"),
            &Method::PUT => format!("\x1b[1;33m{:<8}\x1b[0m", "PUT"),
            &Method::DELETE => format!("\x1b[1;31m{:<8}\x1b[0m", "DELETE"),
            &Method::PATCH => format!("\x1b[1;36m{:<8}\x1b[0m", "PATCH"),
            &Method::HEAD => format!("\x1b[1;35m{:<8}\x1b[0m", "HEAD"),
            &Method::OPTIONS => format!("\x1b[34m{:<8}\x1b[0m", "OPTIONS"),
            &Method::CONNECT => format!("\x1b[35m{:<8}\x1b[0m", "CONNECT"),
            other => format!("\x1b[1;37m{other:<8}\x1b[0m"),
        };
        write!(f, "  {colored_method}  {}", self.path)
    }
}

impl PartialEq<(Method, String)> for RouteInfo {
    fn eq(&self, other: &(Method, String)) -> bool {
        self.method == other.0 && self.path == other.1
    }
}

impl PartialEq<(Method, &str)> for RouteInfo {
    fn eq(&self, other: &(Method, &str)) -> bool {
        self.method == other.0 && self.path == other.1
    }
}

impl PartialEq<RouteInfo> for (Method, &str) {
    fn eq(&self, other: &RouteInfo) -> bool {
        self.0 == other.method && self.1 == other.path
    }
}

impl PartialEq<RouteInfo> for (Method, String) {
    fn eq(&self, other: &RouteInfo) -> bool {
        self.0 == other.method && self.1 == other.path
    }
}

impl RouteInfo {
    /// Creates a new route metadata
    pub(crate) fn new(method: Method, path: &str) -> Self {
        Self { method, path: path.into() }
    }

    pub(crate) fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Method;

    #[test]
    fn it_can_create_route_info() {
        let route = RouteInfo::new(Method::GET, "/health");

        assert_eq!(route.method, Method::GET);
        assert_eq!(route.path, "/health");
    }

    #[test]
    fn it_does_compare_route_info_with_method_and_string() {
        let route = RouteInfo::new(Method::POST, "/users");

        assert_eq!(route, (Method::POST, "/users".to_string()));
        assert_eq!((Method::POST, "/users".to_string()), route);
    }

    #[test]
    fn it_does_compare_route_info_with_method_and_str() {
        let route = RouteInfo::new(Method::PUT, "/users/1");

        assert_eq!(route, (Method::PUT, "/users/1"));
        assert_eq!((Method::PUT, "/users/1"), route);
    }

    #[test]
    fn it_does_not_match_different_method_or_path() {
        let route = RouteInfo::new(Method::GET, "/users");

        assert_ne!(route, (Method::POST, "/users"));
        assert_ne!(route, (Method::GET, "/admins"));
    }

    #[test]
    fn it_can_format_route_info_with_colored_method() {
        let route = RouteInfo::new(Method::GET, "/health");

        let formatted = route.to_string();

        assert!(formatted.contains("/health"));
        assert!(formatted.contains("GET"));
        assert!(formatted.starts_with("  "));
    }

    #[test]
    fn it_can_deref_routes_info_as_vec() {
        let routes = RoutesInfo(vec![
            RouteInfo::new(Method::GET, "/"),
            RouteInfo::new(Method::POST, "/users"),
        ]);

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0], (Method::GET, "/"));
        assert_eq!(routes[1], (Method::POST, "/users"));
    }

    #[test]
    fn it_can_mutate_routes_info_via_deref_mut() {
        let mut routes = RoutesInfo(vec![]);

        routes.push(RouteInfo::new(Method::DELETE, "/users/1"));

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::DELETE, "/users/1"));
    }

    #[test]
    fn it_can_format_routes_info_display() {
        let routes = RoutesInfo(vec![
            RouteInfo::new(Method::GET, "/"),
            RouteInfo::new(Method::POST, "/users"),
        ]);

        let output = routes.to_string();

        assert!(output.contains("Available routes:"));
        assert!(output.contains("/"));
        assert!(output.contains("/users"));
        assert!(output.contains("GET"));
        assert!(output.contains("POST"));
    }

    #[test]
    fn it_can_display_empty_routes_info() {
        let routes = RoutesInfo(vec![]);

        let output = routes.to_string();

        assert!(output.contains("Available routes:"));
    }
}
