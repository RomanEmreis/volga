//! Types and utilities for working with TCP connections.

use std::net::{IpAddr, SocketAddr};

const DEFAULT_PORT: u16 = 7878;

/// Wraps a socket
#[derive(Debug)]
pub struct Connection {
    pub(super) socket: SocketAddr
}

impl Default for Connection {
    #[inline]
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        let ip = [127, 0, 0, 1];
        #[cfg(not(target_os = "windows"))]
        let ip = [0, 0, 0, 0];
        let socket = (ip, DEFAULT_PORT).into();
        Self { socket }
    }
}

impl From<&str> for Connection {
    #[inline]
    fn from(s: &str) -> Self {
        if let Ok(socket) = s.parse::<SocketAddr>() {
            Self { socket }
        } else {
            Self::default()
        }
    }
}

impl From<String> for Connection {
    #[inline]
    fn from(s: String) -> Self {
        if let Ok(socket) = s.parse::<SocketAddr>() {
            Self { socket }
        } else {
            Self::default()
        }
    }
}

impl<I: Into<IpAddr>> From<(I, u16)> for Connection {
    #[inline]
    fn from(value: (I, u16)) -> Self {
        Self { socket: SocketAddr::from(value) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_creates_connection_with_default_socket() {
        let connection = Connection::default();

        #[cfg(target_os = "windows")]
        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }

    #[test]
    fn it_creates_connection_with_specified_socket() {
        let connection: Connection = "127.0.0.1:5000".into();

        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_creates_default_connection_from_empty_str() {
        let connection: Connection = "".into();

        #[cfg(target_os = "windows")]
        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }

    #[test]
    fn it_creates_connection_with_specified_socket_from_tuple() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_debugs_connection() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(format!("{connection:?}"), "Connection { socket: 127.0.0.1:5000 }");
    }

    #[test]
    fn it_sets_default_connection_if_ip_is_invalid() {
        let connection: Connection = "invalid_ip".into();

        #[cfg(target_os = "windows")]
        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }
}