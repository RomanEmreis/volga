//! Main application entry point

use std::{
    future::Future,
    io::Error,
    net::SocketAddr,
    sync::Arc
};
use std::net::IpAddr;
use hyper::rt::{Read, Write};
use hyper_util::rt::TokioIo;

use tokio::{
    io::self,
    net::{TcpListener, TcpStream},
    signal,
    sync::broadcast
};

use crate::server::Server;

use self::{
    pipeline::{Pipeline, PipelineBuilder},
    scope::Scope
};

#[cfg(feature = "di")]
use crate::di::{Container, ContainerBuilder};

#[cfg(feature = "tls")]
use tokio_rustls::TlsAcceptor;

#[cfg(feature = "tls")]
use crate::tls::TlsConfig;

pub mod router;
pub(crate) mod pipeline;
pub(crate) mod scope;

const DEFAULT_PORT: u16 = 7878;

/// The web application used to configure the HTTP pipeline, and routes.
///
/// # Examples
/// ```no_run
///use volga::App;
///
///#[tokio::main]
///async fn main() -> std::io::Result<()> {
///    let mut app = App::new().bind("127.0.0.1:8080");
///    
///    app.run().await
///}
/// ```
pub struct App {
    #[cfg(feature = "di")]
    pub(super) container: ContainerBuilder,
    #[cfg(feature = "tls")]
    pub(super) tls_config: Option<TlsConfig>,
    pub(super) pipeline: PipelineBuilder,
    connection: Connection
}

/// Wraps a socket
pub struct Connection {
    socket: SocketAddr
}

impl Default for Connection {
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
    fn from(s: &str) -> Self {
        if let Ok(socket) = s.parse::<SocketAddr>() {
            Self { socket }
        } else {
            Self::default()
        }
    }
}

impl<I: Into<IpAddr>> From<(I, u16)> for Connection {
    fn from(value: (I, u16)) -> Self {
        Self { socket: SocketAddr::from(value) }
    }
}

/// Contains a shared resources of running Web Server
pub(crate) struct AppInstance {
    #[cfg(feature = "tls")]
    pub(super) acceptor: Option<TlsAcceptor>,
    #[cfg(feature = "di")]
    container: Container,
    pipeline: Pipeline
}

impl TryFrom<App> for AppInstance {
    type Error = Error;

    fn try_from(app: App) -> Result<Self, Self::Error> {
        #[cfg(feature = "tls")]
        let acceptor = {
            let tls_config = app.tls_config
                .map(|config| config.build())
                .transpose()?;
            tls_config
                .map(|config| TlsAcceptor::from(Arc::new(config)))
        };
        let app_instance = Self {
            pipeline: app.pipeline.build(),
            #[cfg(feature = "di")]
            container: app.container.build(),
            #[cfg(feature = "tls")]
            acceptor
        };
        Ok(app_instance)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// General impl
impl App {
    /// Initializes a new instance of the `App` which will be bound to the 127.0.0.1:7878 socket by default.
    /// 
    ///# Examples
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new();
    /// ```
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "di")]
            container: ContainerBuilder::new(),
            #[cfg(feature = "tls")]
            tls_config: None,
            pipeline:PipelineBuilder::new(),
            connection: Default::default()
        }
    }

    /// Binds the `App` to the specified `socket` address.
    /// 
    ///# Examples
    /// ```no_run
    ///use volga::App;
    ///
    ///let app = App::new().bind("127.0.0.1:7878");
    ///let app = App::new().bind(([127,0,0,1], 7878));
    /// ```
    pub fn bind<S: Into<Connection>>(mut self, socket: S) -> Self {
        self.connection = socket.into();
        self
    }

    /// Runs the `App`
    #[cfg(feature = "middleware")]
    pub fn run(mut self) -> impl Future<Output = io::Result<()>> {
        self.use_endpoints();
        self.run_internal()
    }

    /// Runs the `App`
    #[cfg(not(feature = "middleware"))]
    pub fn run(self) -> impl Future<Output = io::Result<()>> {
        self.run_internal()
    }
    
    #[inline]
    async fn run_internal(self) -> io::Result<()> {
        let socket = self.connection.socket;
        let tcp_listener = TcpListener::bind(socket).await?;
        println!("Start listening: {socket}");

        let (shutdown_sender, mut shutdown_signal) = broadcast::channel::<()>(1);
        Self::subscribe_for_ctrl_c_signal(&shutdown_sender);

        #[cfg(feature = "tls")]
        if let Some(tls_config) = &self.tls_config { 
            if tls_config.use_https_redirection {
                Self::run_https_redirection_middleware(
                    socket, 
                    tls_config.http_port,
                    shutdown_sender.subscribe());
            }
        }
        
        let app_instance: Arc<AppInstance> = Arc::new(self.try_into()?);
        loop {
            tokio::select! {
                Ok((stream, _)) = tcp_listener.accept() => {
                    let app_instance = app_instance.clone();
                    tokio::spawn(Self::handle_connection(stream, app_instance));
                }
                _ = shutdown_signal.recv() => {
                    println!("Shutting down server...");
                    break;
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn subscribe_for_ctrl_c_signal(shutdown_sender: &broadcast::Sender<()>) {
        let ctrl_c_shutdown_sender = shutdown_sender.clone();
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(_) => (),
                Err(err) => eprintln!("Unable to listen for shutdown signal: {}", err)
            };

            match ctrl_c_shutdown_sender.send(()) {
                Ok(_) => (),
                Err(err) => eprintln!("Failed to send shutdown signal: {}", err)
            }
        });
    }

    async fn handle_connection(stream: TcpStream, app_instance: Arc<AppInstance>) {
        #[cfg(not(feature = "tls"))]
        Self::serve(TokioIo::new(stream), app_instance).await;
        
        #[cfg(feature = "tls")]
        if let Some(acceptor) = app_instance.acceptor() {
            let stream = match acceptor.accept(stream).await {
                Ok(tls_stream) => tls_stream,
                Err(err) => {
                    eprintln!("failed to perform tls handshake: {err:#}");
                    return;
                }
            };
            let io = TokioIo::new(stream);
            Self::serve(io, app_instance).await;
        } else {
            let io = TokioIo::new(stream);
            Self::serve(io, app_instance).await;
        };
    }

    #[inline]
    async fn serve<I: Read + Write + Unpin>(io: I, app_instance: Arc<AppInstance>) {
        let server = Server::new(io);
        let scope = Scope::new(app_instance);

        server.serve(scope).await;
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use crate::App;
    use crate::app::Connection;

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
    fn it_creates_connection_with_specified_socket_from_tuple() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 5000)));
    }
    
    #[test]
    fn it_creates_app_with_default_socket() {
        let app = App::new();
        
        #[cfg(target_os = "windows")]
        assert_eq!(app.connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(app.connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }

    #[test]
    fn it_binds_app_to_socket() {
        let app = App::new().bind("127.0.0.1:5001");

        assert_eq!(app.connection.socket, SocketAddr::from(([127, 0, 0, 1], 5001)));
    }
}