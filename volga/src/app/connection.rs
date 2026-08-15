//! Types and utilities for working with TCP connections.

use tokio::net::TcpListener;
use std::{
    fmt,
    io::{Error, ErrorKind, Result},
    net::{IpAddr, SocketAddr},
};

const DEFAULT_PORT: u16 = 7878;

/// Maximum length of a host name, in bytes (RFC 1035, Section 2.3.4).
const MAX_HOST_LEN: usize = 253;

/// Maximum length of a single host name label, in bytes (RFC 1035, Section 2.3.4).
const MAX_LABEL_LEN: usize = 63;

/// Describes why a bind address could not be understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddrError {
    /// The address carries no `:port` suffix.
    MissingPort,
    /// The part after the last `:` is not a port number in the 0..=65535 range.
    InvalidPort,
    /// The part before the port is empty.
    MissingHost,
    /// The part before the port is neither an IP literal nor a host name.
    InvalidHost,
}

impl fmt::Display for AddrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::MissingPort => "missing ':port' suffix",
            Self::InvalidPort => "port must be a number in the 0..=65535 range",
            Self::MissingHost => "missing host",
            Self::InvalidHost => "host is neither an IP address nor a host name",
        };
        f.write_str(msg)
    }
}

/// What the server was asked to listen on.
#[derive(Debug)]
enum Target {
    /// A socket address that needs no resolution.
    Addr(SocketAddr),
    /// A host name that is resolved when the server starts.
    Named { host: Box<str>, port: u16 },
    /// An address that could not be understood; reported when the server starts.
    Invalid { input: Box<str>, error: AddrError },
}

/// Wraps a socket
///
/// Addresses are accepted in the same grammar [`tokio::net::TcpListener::bind`] accepts:
/// `127.0.0.1:7878`, `[::1]:7878`, the unbracketed `::1:7878`, and host names such as
/// `localhost:7878`. Host names are resolved when the server starts, never at bind time,
/// and an address that cannot be understood or resolved is reported as an error from
/// [`crate::App::run`] instead of being silently replaced by a different one.
#[derive(Debug)]
pub struct Connection {
    target: Target,
}

impl Default for Connection {
    #[inline]
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        let ip = [127, 0, 0, 1];
        #[cfg(not(target_os = "windows"))]
        let ip = [0, 0, 0, 0];
        Self {
            target: Target::Addr((ip, DEFAULT_PORT).into()),
        }
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.target {
            Target::Addr(addr) => write!(f, "{addr}"),
            Target::Named { host, port } => write!(f, "{host}:{port}"),
            Target::Invalid { input, .. } => f.write_str(input),
        }
    }
}

impl From<&str> for Connection {
    #[inline]
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for Connection {
    #[inline]
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<SocketAddr> for Connection {
    #[inline]
    fn from(addr: SocketAddr) -> Self {
        Self {
            target: Target::Addr(addr),
        }
    }
}

impl<I: Into<IpAddr>> From<(I, u16)> for Connection {
    #[inline]
    fn from(value: (I, u16)) -> Self {
        Self {
            target: Target::Addr(SocketAddr::from(value)),
        }
    }
}

impl Connection {
    /// Parses a bind address, remembering the reason it could not be understood.
    fn parse(s: &str) -> Self {
        let target = match parse_target(s) {
            Ok(target) => target,
            Err(error) => Target::Invalid {
                input: s.into(),
                error,
            },
        };

        Self { target }
    }

    /// Returns the port the server was asked to listen on.
    #[inline]
    #[cfg(feature = "config")]
    fn port(&self) -> u16 {
        match &self.target {
            Target::Addr(addr) => addr.port(),
            Target::Named { port, .. } => *port,
            Target::Invalid { .. } => DEFAULT_PORT,
        }
    }

    /// Returns the same connection with `host` and/or `port` replaced, keeping whichever
    /// half is [`None`].
    ///
    /// # Errors
    /// Returns an error if the resulting address cannot be understood.
    #[cfg(feature = "config")]
    pub(crate) fn rebind(self, host: Option<&str>, port: Option<u16>) -> Result<Self> {
        let connection = match (host, port) {
            (Some(host), port) => Self::parse(&join_host_port(host, port.unwrap_or(self.port()))),
            (None, Some(port)) => self.with_port(port),
            (None, None) => self,
        };

        connection.validate()?;
        Ok(connection)
    }

    /// Returns the same connection listening on a different `port`.
    #[inline]
    #[cfg(feature = "config")]
    fn with_port(self, port: u16) -> Self {
        let target = match self.target {
            Target::Addr(mut addr) => {
                addr.set_port(port);
                Target::Addr(addr)
            }
            Target::Named { host, .. } => Target::Named { host, port },
            invalid => invalid,
        };

        Self { target }
    }

    /// Returns an error if the address cannot be understood.
    #[cfg(feature = "config")]
    fn validate(&self) -> Result<()> {
        match &self.target {
            Target::Invalid { input, error } => Err(invalid_addr(input, *error)),
            _ => Ok(()),
        }
    }

    /// Resolves the address if needed and binds a [`TcpListener`] to it.
    ///
    /// A host name that resolves to several addresses is tried in resolution order, and the
    /// first address that can be bound wins - the same rule [`TcpListener::bind`] follows.
    ///
    /// # Errors
    /// Returns an error if the address cannot be understood, cannot be resolved,
    /// or cannot be bound.
    pub(super) async fn bind(&self) -> Result<TcpListener> {
        let listener = match &self.target {
            Target::Addr(addr) => TcpListener::bind(addr).await,
            Target::Named { host, port } => TcpListener::bind((host.as_ref(), *port)).await,
            Target::Invalid { input, error } => return Err(invalid_addr(input, *error)),
        };

        listener.map_err(|err| Error::new(err.kind(), format!("failed to bind '{self}': {err}")))
    }
}

/// Builds the error reported for an address that could not be understood.
#[inline]
fn invalid_addr(input: &str, error: AddrError) -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        format!("invalid bind address '{input}': {error}"),
    )
}

/// Parses a bind address into a [`Target`].
///
/// Mirrors the socket address grammar of [`std::net::ToSocketAddrs`]: an address literal first,
/// then a split at the last `:` so that unbracketed IPv6 literals and host names are accepted.
fn parse_target(s: &str) -> std::result::Result<Target, AddrError> {
    if let Ok(addr) = s.parse::<SocketAddr>() {
        return Ok(Target::Addr(addr));
    }

    let (host, port) = s.rsplit_once(':').ok_or(AddrError::MissingPort)?;
    let port = port.parse::<u16>().map_err(|_| AddrError::InvalidPort)?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);

    if host.is_empty() {
        return Err(AddrError::MissingHost);
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(Target::Addr(SocketAddr::from((ip, port))));
    }

    if is_host_name(host) {
        Ok(Target::Named {
            host: host.into(),
            port,
        })
    } else {
        Err(AddrError::InvalidHost)
    }
}

/// Checks whether `host` is shaped like a host name that a resolver could look up.
fn is_host_name(host: &str) -> bool {
    // A trailing dot marks a fully qualified name and carries no label of its own.
    let host = host.strip_suffix('.').unwrap_or(host);

    !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL_LEN
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// Joins a host and a port, bracketing IPv6 literals so the result parses back.
#[cfg(feature = "config")]
fn join_host_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_socket() -> SocketAddr {
        #[cfg(target_os = "windows")]
        let ip = [127, 0, 0, 1];
        #[cfg(not(target_os = "windows"))]
        let ip = [0, 0, 0, 0];
        SocketAddr::from((ip, DEFAULT_PORT))
    }

    /// Returns the error a rejected address is reported with, panicking if it was accepted.
    fn rejection(connection: &Connection) -> Error {
        match &connection.target {
            Target::Invalid { input, error } => invalid_addr(input, *error),
            other => panic!("expected a rejected address, got {other:?}"),
        }
    }

    fn addr(connection: &Connection) -> SocketAddr {
        match connection.target {
            Target::Addr(addr) => addr,
            ref other => panic!("expected a socket address, got {other:?}"),
        }
    }

    #[test]
    fn it_creates_connection_with_default_socket() {
        let connection = Connection::default();

        assert_eq!(addr(&connection), default_socket());
    }

    #[test]
    fn it_creates_connection_with_specified_socket() {
        let connection: Connection = "127.0.0.1:5000".into();

        assert_eq!(addr(&connection), SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_creates_connection_from_string() {
        let connection: Connection = String::from("127.0.0.1:5000").into();

        assert_eq!(addr(&connection), SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_creates_connection_with_specified_socket_from_tuple() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(addr(&connection), SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_creates_connection_from_socket_addr() {
        let socket = SocketAddr::from(([127, 0, 0, 1], 5000));
        let connection: Connection = socket.into();

        assert_eq!(addr(&connection), socket);
    }

    #[test]
    fn it_creates_connection_from_bracketed_ipv6() {
        let connection: Connection = "[::1]:3000".into();

        assert_eq!(addr(&connection).to_string(), "[::1]:3000");
    }

    #[test]
    fn it_creates_connection_from_unbracketed_ipv6() {
        let connection: Connection = "::1:3000".into();

        assert_eq!(addr(&connection).to_string(), "[::1]:3000");
    }

    #[test]
    fn it_creates_connection_from_host_name() {
        let connection: Connection = "localhost:3000".into();

        assert!(matches!(
            connection.target,
            Target::Named { ref host, port: 3000 } if host.as_ref() == "localhost"
        ));
    }

    #[test]
    fn it_creates_connection_from_fully_qualified_host_name() {
        let connection: Connection = "api.example.com.:8080".into();

        assert!(matches!(
            connection.target,
            Target::Named { ref host, port: 8080 } if host.as_ref() == "api.example.com."
        ));
    }

    #[test]
    fn it_debugs_connection() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(
            format!("{connection:?}"),
            "Connection { target: Addr(127.0.0.1:5000) }"
        );
    }

    #[test]
    fn it_displays_connection() {
        let socket: Connection = ([127, 0, 0, 1], 5000).into();
        let named: Connection = "localhost:3000".into();
        let invalid: Connection = "invalid_ip".into();

        assert_eq!(socket.to_string(), "127.0.0.1:5000");
        assert_eq!(named.to_string(), "localhost:3000");
        assert_eq!(invalid.to_string(), "invalid_ip");
    }

    #[test]
    fn it_rejects_address_without_port() {
        let connection: Connection = "invalid_ip".into();

        let err = rejection(&connection);
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "invalid bind address 'invalid_ip': missing ':port' suffix"
        );
    }

    #[test]
    fn it_rejects_empty_address() {
        let connection: Connection = "".into();

        let err = rejection(&connection);
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "invalid bind address '': missing ':port' suffix"
        );
    }

    #[test]
    fn it_rejects_address_without_host() {
        let connection: Connection = ":3000".into();

        assert_eq!(
            rejection(&connection).to_string(),
            "invalid bind address ':3000': missing host"
        );
    }

    #[test]
    fn it_rejects_address_with_invalid_port() {
        let connection: Connection = "localhost:70000".into();

        assert_eq!(
            rejection(&connection).to_string(),
            "invalid bind address 'localhost:70000': port must be a number in the 0..=65535 range"
        );
    }

    #[test]
    fn it_rejects_address_with_invalid_host() {
        let connection: Connection = "not a host:3000".into();

        assert_eq!(
            rejection(&connection).to_string(),
            "invalid bind address 'not a host:3000': host is neither an IP address nor a host name"
        );
    }

    #[test]
    fn it_rejects_too_long_host_name() {
        let host = "a".repeat(MAX_HOST_LEN + 1);
        let connection: Connection = format!("{host}:3000").as_str().into();

        assert_eq!(rejection(&connection).kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn it_rejects_too_long_label() {
        let label = "a".repeat(MAX_LABEL_LEN + 1);
        let connection: Connection = format!("{label}.com:3000").as_str().into();

        assert_eq!(rejection(&connection).kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn it_accepts_socket_and_named_addresses() {
        let socket: Connection = "127.0.0.1:3000".into();
        let named: Connection = "localhost:3000".into();

        assert!(matches!(socket.target, Target::Addr(_)));
        assert!(matches!(named.target, Target::Named { .. }));
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_returns_port() {
        let socket: Connection = "127.0.0.1:3000".into();
        let named: Connection = "localhost:4000".into();
        let invalid: Connection = "invalid_ip".into();

        assert_eq!(socket.port(), 3000);
        assert_eq!(named.port(), 4000);
        assert_eq!(invalid.port(), DEFAULT_PORT);
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_replaces_port() {
        let socket: Connection = "127.0.0.1:3000".into();
        let named: Connection = "localhost:3000".into();
        let invalid: Connection = "invalid_ip".into();

        assert_eq!(socket.with_port(9090).to_string(), "127.0.0.1:9090");
        assert_eq!(named.with_port(9090).to_string(), "localhost:9090");
        assert_eq!(invalid.with_port(9090).to_string(), "invalid_ip");
    }

    #[tokio::test]
    async fn it_binds_socket_address() {
        let connection: Connection = "127.0.0.1:0".into();

        let listener = connection.bind().await.unwrap();
        let socket = listener.local_addr().unwrap();

        assert_eq!(socket.ip().to_string(), "127.0.0.1");
        assert_ne!(socket.port(), 0);
    }

    #[tokio::test]
    async fn it_binds_resolved_host_name() {
        let connection: Connection = "localhost:0".into();

        let listener = connection.bind().await.unwrap();

        assert!(
            listener.local_addr().unwrap().ip().is_loopback(),
            "localhost must never bind a non-loopback address"
        );
    }

    #[tokio::test]
    async fn it_does_not_bind_invalid_address() {
        let connection: Connection = "localhost".into();

        let err = connection.bind().await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "invalid bind address 'localhost': missing ':port' suffix"
        );
    }

    #[tokio::test]
    async fn it_does_not_bind_unresolvable_host_name() {
        let connection: Connection = "volga.invalid:0".into();

        let err = connection.bind().await.unwrap_err();

        assert!(
            err.to_string()
                .starts_with("failed to bind 'volga.invalid:0'"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn it_reports_address_in_use_with_the_address() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let connection: Connection = format!("127.0.0.1:{port}").into();
        let err = connection.bind().await.unwrap_err();

        assert!(
            err.to_string()
                .starts_with(&format!("failed to bind '127.0.0.1:{port}'")),
            "unexpected error: {err}"
        );
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_host_only() {
        let connection: Connection = "127.0.0.1:3000".into();

        let connection = connection.rebind(Some("0.0.0.0"), None).unwrap();

        assert_eq!(connection.to_string(), "0.0.0.0:3000");
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_port_only() {
        let connection: Connection = "localhost:3000".into();

        let connection = connection.rebind(None, Some(9090)).unwrap();

        assert_eq!(connection.to_string(), "localhost:9090");
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_host_and_port() {
        let connection: Connection = "127.0.0.1:3000".into();

        let connection = connection.rebind(Some("localhost"), Some(9090)).unwrap();

        assert_eq!(connection.to_string(), "localhost:9090");
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_nothing() {
        let connection: Connection = "127.0.0.1:3000".into();

        let connection = connection.rebind(None, None).unwrap();

        assert_eq!(connection.to_string(), "127.0.0.1:3000");
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_ipv6_host() {
        let connection: Connection = "127.0.0.1:3000".into();

        let connection = connection.rebind(Some("::1"), Some(9090)).unwrap();

        assert_eq!(connection.to_string(), "[::1]:9090");
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_does_not_rebind_invalid_host() {
        let connection: Connection = "127.0.0.1:3000".into();

        let err = connection
            .rebind(Some("not a host"), Some(9090))
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "invalid bind address 'not a host:9090': host is neither an IP address nor a host name"
        );
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_joins_host_and_port() {
        assert_eq!(join_host_port("localhost", 80), "localhost:80");
        assert_eq!(join_host_port("127.0.0.1", 80), "127.0.0.1:80");
        assert_eq!(join_host_port("::1", 80), "[::1]:80");
        assert_eq!(join_host_port("[::1]", 80), "[::1]:80");
    }
}
