//! Helpers shared by the e2e test suites: a real volga application bound
//! to a free localhost port.

use std::{fmt, net::TcpListener};
use volga::App;

/// A free localhost port, held open until a test server takes it.
///
/// Picking a port by binding one and letting it go leaves a window in which a test running
/// alongside can pick the same port. When both then serve on it, one bind fails - and a test
/// whose server never started talks to the other test's application instead, which is how a
/// request answers with a `404` it was never mapped to. Holding the socket from the moment the
/// port is picked leaves no such window.
///
/// Displayed as the port number, for building URLs.
pub(crate) struct Port {
    listener: TcpListener,
    number: u16,
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.number.fmt(f)
    }
}

/// Grabs a free localhost port.
///
/// Nothing accepts connections on it until it is passed to [`serve`], and dropping it frees
/// the port.
pub(crate) fn free_port() -> Port {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let number = listener.local_addr().unwrap().port();

    Port { listener, number }
}

/// Spawns `app` on `port`.
///
/// The socket is already listening, so a request sent before the server task starts waits in
/// the backlog rather than being refused.
pub(crate) async fn serve(port: Port, app: App) -> tokio::task::JoinHandle<()> {
    let app = app.without_greeter();

    tokio::spawn(async move {
        let _ = app.run_with_std_listener(port.listener).await;
    })
}
