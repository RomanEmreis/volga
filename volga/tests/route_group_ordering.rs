#![allow(missing_docs)]
#![cfg(all(feature = "middleware", feature = "tracing"))]

use std::{
    io,
    sync::{Arc, Mutex},
};
use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;
use volga::App;

/// Collects everything a `tracing` subscriber writes so a test can assert on it.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl LogBuffer {
    fn contents(&self) -> String {
        let bytes = self.0.lock().expect("log buffer is not poisoned").clone();
        String::from_utf8(bytes).expect("log output is valid UTF-8")
    }
}

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("poisoned log buffer"))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogBuffer {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Runs `setup` with a subscriber that captures warnings and returns what it logged.
fn capture_warnings<F: FnOnce(&mut App)>(setup: F) -> String {
    let buffer = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buffer.clone())
        .with_max_level(Level::WARN)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let mut app = App::new();
        setup(&mut app);
    });

    buffer.contents()
}

#[test]
fn it_warns_when_group_middleware_is_registered_after_a_route() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.wrap(|ctx, next| async move { next(ctx).await });
        });
    });

    assert!(
        logs.contains("RouteGroup::wrap must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
    assert!(
        logs.contains("1 route(s) already mapped under '/api' will not be affected"),
        "unexpected logs: {logs}"
    );
}

#[test]
fn it_does_not_warn_when_group_middleware_is_registered_before_a_route() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.wrap(|ctx, next| async move { next(ctx).await });
            api.map_get("/hello", || async { "Hello, World!" });
        });
    });

    assert!(logs.is_empty(), "unexpected logs: {logs}");
}

#[test]
fn it_warns_when_group_middleware_is_registered_after_a_sub_group() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.group("/users", |users| {
                users.map_get("/{id}", || async { "Hello, World!" });
            });
            api.filter(|| async { true });
        });
    });

    assert!(
        logs.contains("RouteGroup::filter must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
}

#[test]
fn it_warns_when_group_cors_is_registered_after_a_route() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.disable_cors();
        });
    });

    assert!(
        logs.contains("RouteGroup::disable_cors must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
}

/// Group-level helpers are thin wrappers over `attach` / `wrap` / `map_ok` / `map_err`,
/// so the warning has to name the helper the caller actually wrote, once.
#[cfg(feature = "rate-limiting")]
#[test]
fn it_names_the_rate_limiting_method_that_came_too_late() {
    use volga::rate_limiting::by;

    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.token_bucket(by::ip());
        });
    });

    assert!(
        logs.contains("RouteGroup::token_bucket must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
    assert!(
        !logs.contains("RouteGroup::attach"),
        "the warning should not name the method the caller went through: {logs}"
    );
    assert_eq!(logs.lines().count(), 1, "unexpected logs: {logs}");
}

#[cfg(feature = "problem-details")]
#[test]
fn it_names_the_error_handler_that_came_too_late() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.map_problem();
        });
    });

    assert!(
        logs.contains("RouteGroup::map_problem must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
    assert!(
        !logs.contains("RouteGroup::map_err"),
        "unexpected logs: {logs}"
    );
}

#[test]
fn it_names_the_response_mapping_that_came_too_late() {
    let logs = capture_warnings(|app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.cache_control(|cache_control| cache_control.with_max_age(60));
        });
    });

    assert!(
        logs.contains("RouteGroup::cache_control must be called before any map_* in the group"),
        "unexpected logs: {logs}"
    );
    assert!(
        !logs.contains("RouteGroup::map_ok"),
        "unexpected logs: {logs}"
    );
}
