use std::sync::Arc;
use std::future::Future;
use std::time::Duration;
use bytes::BytesMut;
use http::header::{
    CONTENT_LENGTH, 
    CONTENT_TYPE
};
use http::{HeaderMap, HeaderValue};
use http_body_util::BodyExt;
use tokio::{
    net::{TcpListener, TcpStream},
    io::{self, Error, AsyncReadExt, AsyncWriteExt, BufReader, Interest},
    sync::broadcast,
    signal,
    sync::Mutex
};
use tokio::io::ErrorKind::{
    InvalidData,
    InvalidInput,
    BrokenPipe,
};
use tokio_util::{
    sync::CancellationToken,
};
use crate::app::{
    endpoints::{Endpoints, EndpointContext},
    middlewares::{Middlewares, mapping::asynchronous::AsyncMiddlewareMapping},
    request::{RawRequest, HttpRequest},
    results::{Results, HttpResponse, HttpResult}
};

pub mod middlewares;
pub mod endpoints;
pub mod body;
pub mod request;
pub mod results;
pub mod mapping;

/// The web application used to configure the HTTP pipeline, and routes.
///
/// # Examples
/// ```no_run
///use volga::App;
///
///#[tokio::main]
///async fn main() -> std::io::Result<()> {
///    let mut app = App::build("127.0.0.1:7878").await?;
///    
///    app.run().await
///}
/// ```
pub struct App {
    pipeline: Pipeline,
    connection: Connection
}

struct Pipeline {
    middlewares: Middlewares,
    endpoints: Endpoints
}

struct Connection {
    tcp_listener: TcpListener,
    shutdown_signal: broadcast::Receiver<()>,
    shutdown_sender: broadcast::Sender<()>
}

pub struct HttpContext {
    pub request: Mutex<Option<HttpRequest>>,
    endpoint_context: EndpointContext
}

pub(crate) type BoxedHttpResultFuture = Box<dyn Future<Output = HttpResult> + Send>;

impl HttpContext {
    #[inline]
    async fn execute(&self) -> HttpResult {
        let mut request_guard = self.request.lock().await;
        if let Some(request) = request_guard.take() {
            drop(request_guard);
            self.endpoint_context.handler.call(request).await    
        } else {
            Results::internal_server_error(None)            
        }
    }
}

impl App {
    #[inline]
    pub(crate) fn middlewares(&mut self) -> &mut Middlewares {
        &mut self.pipeline.middlewares
    }

    #[inline]
    pub(crate) fn endpoints(&mut self) -> &mut Endpoints {
        &mut self.pipeline.endpoints
    }
    
    /// Initializes a new instance of the `App` on specified `socket`.
    /// 
    ///# Examples
    /// ```no_run
    ///use volga::App;
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::build("127.0.0.1:7878").await?;
    ///    
    ///    app.run().await
    ///}
    /// ```
    pub async fn build(socket: &str) -> io::Result<App> {
        if socket.is_empty() {
            return Err(Error::new(InvalidData, "An empty socket has been provided."));
        }

        let tcp_listener = TcpListener::bind(socket).await?;
        let (shutdown_sender, shutdown_receiver) = broadcast::channel::<()>(1);

        Self::subscribe_for_ctrl_c_signal(&shutdown_sender);
        
        let connection = Connection { 
            tcp_listener, 
            shutdown_sender, 
            shutdown_signal: shutdown_receiver
        };
        
        let pipeline = Pipeline { 
            middlewares: Middlewares::new(),
            endpoints: Endpoints::new()
        }; 
        
        let server = Self {
            connection,
            pipeline
        };

        println!("Start listening: {socket}");
        
        Ok(server)
    }

    /// Runs the Web Server
    pub async fn run(mut self) -> io::Result<()> {
        self.use_endpoints();

        let connection = &mut self.connection;
        let pipeline = Arc::new(self.pipeline);
        
        loop {
            tokio::select! {
                Ok((socket, _)) = connection.tcp_listener.accept() => {
                    let pipeline = pipeline.clone();
                    
                    tokio::spawn(async move {
                        Self::handle_connection(&pipeline, socket).await;
                    });
                }
                _ = connection.shutdown_signal.recv() => {
                    println!("Shutting down server...");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Gracefully shutdown the server
    pub fn shutdown(&self) {
        match self.connection.shutdown_sender.send(()) {
            Ok(_) => (),
            Err(err) => {
                eprintln!("Failed to send shutdown the server: {}", err);
            }
        };
    }

    #[inline]
    fn subscribe_for_ctrl_c_signal(shutdown_sender: &broadcast::Sender<()>) {
        let ctrl_c_shutdown_sender = shutdown_sender.clone();
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(_) => (),
                Err(err) => {
                    eprintln!("Unable to listen for shutdown signal: {}", err);
                }
            };

            match ctrl_c_shutdown_sender.send(()) {
                Ok(_) => (),
                Err(err) => {
                    eprintln!("Failed to send shutdown signal: {}", err);
                }
            }
        });
    }
    
    #[inline]
    fn use_endpoints(&mut self) {
        self.use_middleware(|ctx, _| async move {
            ctx.execute().await
        });
    }

    #[inline]
    async fn handle_connection(pipeline: &Arc<Pipeline>, mut socket: TcpStream) {
        let mut buffer = BytesMut::with_capacity(4096);
        
        loop {
            match Self::handle_request(pipeline, &mut socket, &mut buffer).await {
                Ok(response) => {
                    if let Err(err) = Self::write_response(&mut socket, response).await {
                        if cfg!(debug_assertions) {
                            eprintln!("Failed to write to socket: {:?}", err);
                        }
                        break; // Break the loop if fail to write to the socket
                    }
                }
                Err(err) => {
                    if cfg!(debug_assertions) {
                        eprintln!("Error occurred handling request: {}", err);
                    }
                    break; // Break the loop if handle_request returns an error
                }
            }
        }
    }
    
    async fn create_cancellation_monitoring_task(socket: &mut TcpStream) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            match socket.ready(Interest::READABLE | Interest::WRITABLE).await {
                Ok(ready) if ready.is_read_closed() || ready.is_write_closed() => break,
                Ok(_) => continue,
                Err(_) => break
            }
        }
    }

    async fn handle_request(pipeline: &Arc<Pipeline>, socket: &mut TcpStream, buffer: &mut BytesMut) -> io::Result<HttpResponse> {
        let mut buf_reader = BufReader::new(socket);
        buffer.clear();
        
        let bytes_read = buf_reader.read_buf(buffer).await?;
        if bytes_read == 0 {
            return Err(Error::new(BrokenPipe, "Client closed the connection"));
        }
        
        let mut http_request = RawRequest::parse(&buffer[..bytes_read])?;
        let cancellation_token = CancellationToken::new();

        let socket = buf_reader.into_inner();
        
        if let Some(endpoint_context) = pipeline.endpoints.get_endpoint(&http_request).await {
            let extensions = http_request.extensions_mut();
            extensions.insert(cancellation_token.clone());

            if !endpoint_context.params.is_empty() {
                extensions.insert(endpoint_context.params.clone());   
            }

            let context = HttpContext {
                request: Mutex::new(http_request.into()),
                endpoint_context
            };
                
            let response = tokio::select! {
                response = pipeline.middlewares.execute(Arc::new(context)) => response,
                _ = Self::create_cancellation_monitoring_task(socket) => {
                    cancellation_token.cancel();
                    Results::client_closed_request()
                }
            };
            
            match response {
                Ok(response) => Ok(response),
                Err(error) if error.kind() == InvalidInput => Results::bad_request(Some(error.to_string())),
                Err(error) => Results::internal_server_error(Some(error.to_string()))
            }
        } else {
            Results::not_found()
        }
    }

    async fn write_response(socket: &mut TcpStream, response: HttpResponse) -> io::Result<()> {
        let (parts, mut body) = response.into_parts();
        
        let mut response_bytes = BytesMut::new();
        // Start with the HTTP status line
        let status_line = format!(
            "HTTP/1.1 {} {}\r\n",
            &parts.status.as_u16(),
            &parts.status.canonical_reason().unwrap_or("unknown status")
        );
        response_bytes.extend_from_slice(status_line.as_bytes());
                
        // Write headers
        for (key, value) in &parts.headers {
            let header_value = match value.to_str() {
                Ok(v) => v,
                Err(_) => return Err(Error::new(InvalidData, "Invalid header value")),
            };
            let header = format!("{}: {}\r\n", key, header_value);
            response_bytes.extend_from_slice(header.as_bytes());
        }

        // End of headers section
        response_bytes.extend_from_slice(b"\r\n");

        socket.write_all(&response_bytes).await?;
        
        let (content_length, content_type) = Self::get_response_metadata(&parts.headers);
        
        if content_length > 0 {
            if content_type == mime::APPLICATION_OCTET_STREAM {
                while let Some(next) = body.frame().await {
                    let frame = next?;
                    if let Some(chunk) = frame.data_ref() {
                        socket.write_all(chunk).await?;
                    }
                }
            } else {
                let bytes = body.collect().await?.to_bytes();
                socket.write_all(&bytes).await?;
            }
        }
        
        socket.flush().await
    }
    
    #[inline]
    fn get_response_metadata(headers: &HeaderMap<HeaderValue>) -> (usize, &str) {
        let content_length = headers.get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|string| string.parse::<usize>().ok())
            .unwrap_or(0);

        let content_type = headers.get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or(mime::APPLICATION_JSON.as_ref());

        (content_length, content_type)
    }
}