//! Types and utilities for working with TCP connections.

use std::{
    fmt,
    io::{Error, ErrorKind, Result},
    net::{IpAddr, SocketAddr},
};
use tokio::net::TcpListener;

const DEFAULT_PORT: u16 = 7878;

/// Maximum length of a host name, in bytes (RFC 1035, Section 2.3.4).
const MAX_HOST_LEN: usize = 253;

/// Describes why a bind address could not be understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddrError {
    /// The address carries no `:port` suffix.
    MissingPort,
    /// The part after the last `:` is not a port number in the 0..=65535 range.
    InvalidPort,
    /// The part before the port is empty.
    MissingHost,
    /// The part before the port is neither an IP literal nor anything a resolver could
    /// look up.
    InvalidHost,
}

impl fmt::Display for AddrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::MissingPort => "missing ':port' suffix",
            Self::InvalidPort => "port must be a number in the 0..=65535 range",
            Self::MissingHost => "missing host",
            Self::InvalidHost => "host is neither an IP address nor a name a resolver could take",
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
/// `127.0.0.1:7878`, `[::1]:7878`, the unbracketed `::1:7878`, zone-scoped IPv6 literals such
/// as `[fe80::1%eth0]:7878`, and host names such as `localhost:7878`. Names - a zone-scoped
/// literal counts as one - are resolved when the server starts, never at bind time,
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
            // A zone-scoped literal is the only name that carries `:` - bracket it, the way
            // `SocketAddr` renders IPv6.
            Target::Named { host, port } if host.contains(':') => write!(f, "[{host}]:{port}"),
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
            (Some(host), port) => Self::from_parts(host, port.unwrap_or(self.port())),
            (None, Some(port)) => self.with_port(port),
            (None, None) => self,
        };

        connection.validate()?;
        Ok(connection)
    }

    /// Parses a host and a port given separately, without joining them into an address first.
    #[cfg(feature = "config")]
    fn from_parts(host: &str, port: u16) -> Self {
        let target = parse_host(host, port).unwrap_or_else(|error| Target::Invalid {
            input: host.into(),
            error,
        });

        Self { target }
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
    /// or cannot be bound. An error the OS reported is passed through as it is, so
    /// [`Error::raw_os_error`] and its source survive.
    pub(super) async fn bind(&self) -> Result<TcpListener> {
        let listener = match &self.target {
            Target::Addr(addr) => TcpListener::bind(addr).await,
            Target::Named { host, port } => TcpListener::bind((host.as_ref(), *port)).await,
            Target::Invalid { input, error } => return Err(invalid_addr(input, *error)),
        };

        listener.map_err(|err| match err.raw_os_error() {
            // Wrapping an OS error would drop its code and its source, and the caller already
            // knows which address it asked for - hand it back untouched.
            Some(_) => err,
            // A resolution failure names neither the host nor an OS code, so it is the one
            // worth the address it failed on.
            None => Error::new(err.kind(), format!("failed to bind '{self}': {err}")),
        })
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

    parse_host(host, port)
}

/// Parses a host - an IP literal, bracketed or not, or a name - against a known port.
fn parse_host(host: &str, port: u16) -> std::result::Result<Target, AddrError> {
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

    // Anything else - a name, or an IPv6 literal carrying a zone id, which `Ipv6Addr` has
    // nowhere to parse into - is handed to the resolver as it is.
    if is_resolvable_host(host) {
        Ok(Target::Named {
            host: host.into(),
            port,
        })
    } else {
        Err(AddrError::InvalidHost)
    }
}

/// Checks whether `host` is shaped like something a resolver could be asked to look up.
///
/// Whether the name *exists* is the resolver's call, not this function's: `/etc/hosts` and
/// other NSS sources define names outside the RFC 1035 preferred syntax (`api+blue`), and
/// [`crate::App::bind`] promises the reach of [`std::net::ToSocketAddrs`]. Only what no
/// resolver could look up is rejected here - an empty host, one longer than a DNS name may be
/// (RFC 1035, Section 2.3.4), or one carrying whitespace or control characters, which marks a
/// typo rather than a name.
fn is_resolvable_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && !host
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
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
    fn it_creates_connection_from_zone_scoped_ipv6() {
        let connection: Connection = "fe80::1%eth0:8080".into();

        assert!(matches!(
            connection.target,
            Target::Named { ref host, port: 8080 } if host.as_ref() == "fe80::1%eth0"
        ));
    }

    #[test]
    fn it_creates_connection_from_bracketed_zone_scoped_ipv6() {
        let connection: Connection = "[fe80::1%eth0]:8080".into();

        assert!(
            matches!(
                connection.target,
                Target::Named { ref host, port: 8080 } if host.as_ref() == "fe80::1%eth0"
            ),
            "the brackets must be stripped - the resolver takes the bare literal"
        );
    }

    #[test]
    fn it_creates_connection_from_bracketed_numeric_zone_scoped_ipv6() {
        let connection: Connection = "[::1%1]:8080".into();

        assert_eq!(
            addr(&connection).to_string(),
            "[::1%1]:8080",
            "a bracketed numeric scope id is an address literal - it needs no resolution"
        );
    }

    #[test]
    fn it_creates_connection_from_unbracketed_numeric_zone_scoped_ipv6() {
        let connection: Connection = "::1%1:8080".into();

        assert!(matches!(
            connection.target,
            Target::Named { ref host, port: 8080 } if host.as_ref() == "::1%1"
        ));
    }

    #[test]
    fn it_displays_zone_scoped_ipv6_bracketed() {
        let connection: Connection = "fe80::1%eth0:8080".into();

        assert_eq!(connection.to_string(), "[fe80::1%eth0]:8080");
    }

    #[test]
    fn it_leaves_an_unusual_zone_to_the_resolver() {
        // Neither a zone on IPv4 nor an empty one is an address this crate can rule out - the
        // resolver owns that call, and reports it as a bind error at startup.
        for input in ["127.0.0.1%eth0:8080", "fe80::1%:8080"] {
            let connection: Connection = input.into();

            assert!(
                matches!(connection.target, Target::Named { .. }),
                "'{input}' must be handed to the resolver"
            );
        }
    }

    #[test]
    fn it_rejects_a_zone_with_whitespace() {
        let connection: Connection = "fe80::1%bad zone:8080".into();

        assert_eq!(rejection(&connection).kind(), ErrorKind::InvalidInput);
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
            "invalid bind address 'not a host:3000': host is neither an IP address nor a name a resolver could take"
        );
    }

    #[test]
    fn it_accepts_a_name_outside_the_preferred_syntax() {
        // `/etc/hosts` and other NSS sources define names RFC 1035 would not - the resolver
        // decides whether they exist, not the parser.
        let connection: Connection = "api+blue:3000".into();

        assert!(matches!(
            connection.target,
            Target::Named { ref host, port: 3000 } if host.as_ref() == "api+blue"
        ));
    }

    #[test]
    fn it_rejects_too_long_host_name() {
        let host = "a".repeat(MAX_HOST_LEN + 1);
        let connection: Connection = format!("{host}:3000").as_str().into();

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

        // A resolver error carries no OS code and does not name the host, so it is annotated;
        // a platform that reports resolution failures as OS errors keeps its own error.
        assert!(
            err.raw_os_error().is_some() || err.to_string().contains("volga.invalid:0"),
            "an unresolvable name must be reported as an OS error or with the address: {err}"
        );
    }

    #[tokio::test]
    async fn it_preserves_the_os_error_when_the_address_is_in_use() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let connection: Connection = format!("127.0.0.1:{port}").into();
        let err = connection.bind().await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::AddrInUse);
        assert!(
            err.raw_os_error().is_some(),
            "the OS error code must survive, so callers can tell platform failures apart: {err}"
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
            "invalid bind address 'not a host': host is neither an IP address nor a name a resolver could take",
            "a config host is reported on its own, without a port glued to it"
        );
    }

    #[cfg(feature = "config")]
    #[test]
    fn it_rebinds_bracketed_and_zone_scoped_hosts() {
        let connection: Connection = "127.0.0.1:3000".into();
        assert_eq!(
            connection
                .rebind(Some("[::1]"), Some(80))
                .unwrap()
                .to_string(),
            "[::1]:80"
        );

        let connection: Connection = "127.0.0.1:3000".into();
        assert_eq!(
            connection
                .rebind(Some("fe80::1%eth0"), Some(80))
                .unwrap()
                .to_string(),
            "[fe80::1%eth0]:80"
        );
    }

    #[tokio::test]
    async fn it_binds_a_zone_scoped_loopback_when_the_platform_has_one() {
        // Interface index 1 is the loopback on the platforms CI runs on, but that is a
        // convention rather than a guarantee - only assert once the bind succeeds.
        let connection: Connection = "::1%1:0".into();

        if let Ok(listener) = connection.bind().await {
            assert!(listener.local_addr().unwrap().ip().is_loopback());
        }
    }
}
