//! Common test utilities

#![allow(missing_docs)]
#![allow(unreachable_pub)]

use volga::App;
use std::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct TestServer {
    pub port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    pub async fn spawn<F>(setup: F) -> Self
    where
        F: FnOnce(&mut App) + Send + 'static,
    {
        let port = Self::get_free_port();
        let (tx, rx) = oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let mut app = App::new().bind(format!("127.0.0.1:{}", port));
            setup(&mut app);

            tokio::select! {
                _ = app.run() => {},
                _ = rx => {}
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Self {
            port,
            shutdown_tx: Some(tx),
            server_handle: Some(server_handle),
        }
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

