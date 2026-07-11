//! Helpers shared by the e2e test suites: a real volga application bound
//! to a free localhost port.

use std::time::Duration;
use volga::App;

/// Grabs a free localhost port.
pub(crate) fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Spawns `app` bound to `port` and waits until it accepts connections.
pub(crate) async fn serve(port: u16, app: App) -> tokio::task::JoinHandle<()> {
    let app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    let handle = tokio::spawn(async move {
        let _ = app.run().await;
    });
    for _ in 0..200 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return handle;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("test server did not start on port {port}");
}
