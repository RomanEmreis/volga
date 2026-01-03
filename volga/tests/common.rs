//! Common test utilities

#![allow(missing_docs)]
#![allow(unreachable_pub)]
#![allow(dead_code)]
#![allow(missing_debug_implementations)]

use volga::App;
use std::net::TcpListener;
use tokio::sync::oneshot;

type AppSetupFn = Box<dyn FnOnce(App) -> App + Send>;
type ServerSetupFn = Box<dyn FnOnce(&mut App) + Send>;

pub struct TestServerBuilder {
    app_config: Option<AppSetupFn>,
    routes: Vec<ServerSetupFn>,
}

impl Default for TestServerBuilder {
    fn default() -> Self {
        Self::new()
    } 
}

pub struct TestServer {
    pub port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestServerBuilder {
    pub fn new() -> Self {
        Self {
            app_config: None,
            routes: Vec::new(),
        }
    }

    pub fn with_app<F>(mut self, config: F) -> Self
    where
        F: FnOnce(App) -> App + Send + 'static,
    {
        self.app_config = Some(Box::new(config));
        self
    }

    pub fn setup<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut App) + Send + 'static,
    {
        self.routes.push(Box::new(f));
        self
    }

    pub async fn build(self) -> TestServer {
        let port = TestServer::get_free_port();
        let (tx, rx) = oneshot::channel();

        let app_config = self.app_config;
        let routes = self.routes;

        let server_handle = tokio::spawn(async move {
            let mut app = App::new()
                .bind(format!("127.0.0.1:{}", port))
                .without_greeter();

            if let Some(config) = app_config {
                app = config(app);
            }

            for route in routes {
                route(&mut app);
            }

            tokio::select! {
                _ = app.run() => {},
                _ = rx => {}
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        TestServer {
            port,
            shutdown_tx: Some(tx),
            server_handle: Some(server_handle),
        }
    }
}

impl TestServer {
    #[inline]
    pub fn builder() -> TestServerBuilder {
        TestServerBuilder::new()
    }

    #[inline]
    pub async fn spawn<F>(setup: F) -> Self
    where
        F: FnOnce(&mut App) + Send + 'static,
    {
        TestServerBuilder::new()
            .setup(setup)
            .build()
            .await
    }

    fn get_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    pub fn client(&self) -> reqwest::Client {
        if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.server_handle.take() {
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_secs(5),
                handle
            ).await;
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

