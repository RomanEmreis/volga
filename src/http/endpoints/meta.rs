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
}