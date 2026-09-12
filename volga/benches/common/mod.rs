//! Shared harness for the Volga benchmarks.
//!
//! The goal of this module is to make every benchmark measure the *framework*
//! flow and as little else as possible. Concretely it:
//!
//! - binds an ephemeral port per benchmark binary, so runs never collide with a
//!   leftover socket from a previous run;
//! - runs the server on its own runtime and its own OS thread, so the client
//!   does not steal the server's worker;
//! - builds a single `reqwest::Client` up front and keeps exactly one pooled
//!   keep-alive connection alive for the whole run;
//! - drains every response body, which is what actually returns the connection
//!   to the pool - without it every request needs a fresh TCP connection;
//! - uses a fixed concurrency that does not drift with criterion's sample size,
//!   so the reported mean is stable instead of creeping up as samples grow;
//! - panics on a transport error or an unexpected status, so a broken run fails
//!   loudly instead of quietly reporting the latency of connection errors.
//!
//! Two measurement modes are available:
//!
//! - [`Harness::get`] sends requests one at a time over a single warm
//!   connection. That is a true end-to-end latency, but on loopback it is
//!   dominated by the round trip (~36 us here), which buries the framework's
//!   own cost below the noise floor.
//! - [`Harness::get_saturated`] keeps a fixed number of requests in flight over
//!   as many pooled connections. With [`Profile::SINGLE`] the server runs on a
//!   single worker thread and the client on four, so the server is deliberately
//!   the bottleneck: the round trip is overlapped away and `elapsed / iters` is
//!   the server's mean per-request service time - the framework flow itself.
//!   [`Profile::MULTI`] gives the server several workers, which is what it takes
//!   for per-request writes to state every worker shares to collide at all.
//!
//! [`Harness::baseline`] spawns a bare hyper server serving a fixed response,
//! under the identical client and runtime setup. Subtracting it from a Volga
//! route leaves the framework cost with the client, the loopback and hyper's own
//! I/O taken out.

#![allow(missing_docs, dead_code)]

use criterion::Bencher;
use futures_util::future::join_all;
use reqwest::{Client, RequestBuilder};
use std::{
    convert::Infallible,
    net::{SocketAddr, TcpListener},
    thread,
};
use tokio::{
    runtime::{Builder, Runtime},
    time::Instant,
};
use volga::App;

use bytes::Bytes;
use http_body_util::Full;
use hyper::{Response, service::service_fn};
use hyper_util::rt::TokioIo;

/// Requests sent before the timer starts: opens the pooled connections and lets
/// the branch predictors and allocator caches settle.
const WARMUP_REQUESTS: usize = 512;

/// How hard a harness drives its server.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Profile {
    /// Worker threads the server runs on.
    pub(crate) server_workers: usize,
    /// Worker threads the client runs on. The client must not become the
    /// bottleneck; hyper drives each pooled connection on its own task, so these
    /// spread across the pool.
    pub(crate) client_threads: usize,
    /// Requests kept in flight by [`Harness::run_saturated`], and the size of the
    /// connection pool: enough to keep the server busy, few enough that the
    /// connections are reused instead of churning through ephemeral ports.
    pub(crate) concurrency: u64,
}

impl Profile {
    /// One server worker kept saturated: per-request service time.
    pub(crate) const SINGLE: Self = Self {
        server_workers: 1,
        client_threads: 4,
        concurrency: 32,
    };

    /// Eight server workers taking requests at once: what per-request writes to
    /// state shared by every worker add up to once they start to collide.
    pub(crate) const MULTI: Self = Self {
        server_workers: 8,
        client_threads: 10,
        concurrency: 128,
    };
}

/// The body every route in the benchmarks returns, so route shapes stay
/// comparable to each other and to the bare-hyper baseline.
pub(crate) const BODY: &str = "Hello, World!";

/// A running server plus the client used to drive it.
pub(crate) struct Harness {
    rt: Runtime,
    client: Client,
    addr: SocketAddr,
    profile: Profile,
}

impl Harness {
    /// Spawns a Volga app with the standard benchmark configuration.
    pub(crate) fn new<S>(setup: S) -> Self
    where
        S: FnOnce(&mut App) + Send + 'static,
    {
        Self::with_config(|app| app, setup)
    }

    /// Spawns a Volga app, allowing extra builder-level configuration.
    pub(crate) fn with_config<C, S>(configure: C, setup: S) -> Self
    where
        C: FnOnce(App) -> App + Send + 'static,
        S: FnOnce(&mut App) + Send + 'static,
    {
        Self::with_profile(Profile::SINGLE, configure, setup)
    }

    /// Spawns a Volga app driven the way `profile` says.
    pub(crate) fn with_profile<C, S>(profile: Profile, configure: C, setup: S) -> Self
    where
        C: FnOnce(App) -> App + Send + 'static,
        S: FnOnce(&mut App) + Send + 'static,
    {
        let addr = serve(profile.server_workers, move |listener| async move {
            let mut app = configure(
                App::new()
                    .with_no_delay()
                    .without_body_limit()
                    .without_greeter(),
            );
            setup(&mut app);
            _ = app.run_with_std_listener(listener).await;
        });
        Self::attach(addr, profile)
    }

    /// Spawns a bare hyper server answering every request with [`BODY`].
    ///
    /// This is the floor: client, loopback TCP and hyper, with no Volga in the
    /// path. Volga timings minus this one are the framework's own cost.
    pub(crate) fn baseline() -> Self {
        Self::baseline_with(Profile::SINGLE)
    }

    /// [`Harness::baseline`], driven the way `profile` says.
    pub(crate) fn baseline_with(profile: Profile) -> Self {
        let addr = serve(profile.server_workers, |listener| async move {
            listener
                .set_nonblocking(true)
                .expect("set_nonblocking failed");
            let listener =
                tokio::net::TcpListener::from_std(listener).expect("failed to adopt the listener");
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                _ = stream.set_nodelay(true);
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let service = service_fn(|_req| async {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .header("content-type", "text/plain; charset=utf-8")
                                .body(Full::new(Bytes::from_static(BODY.as_bytes())))
                                .expect("failed to build the baseline response"),
                        )
                    });
                    #[cfg(all(feature = "http1", not(feature = "http2")))]
                    let served = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await;
                    #[cfg(feature = "http2")]
                    let served = hyper::server::conn::http2::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .serve_connection(io, service)
                    .await;
                    _ = served;
                });
            }
        });
        Self::attach(addr, profile)
    }

    fn attach(addr: SocketAddr, profile: Profile) -> Self {
        let rt = Builder::new_multi_thread()
            .worker_threads(profile.client_threads)
            .enable_all()
            .build()
            .expect("failed to build the client runtime");
        Self {
            rt,
            client: client(profile),
            addr,
            profile,
        }
    }

    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Benchmarks a `GET` against `path` one request at a time (end-to-end
    /// latency), asserting the response status.
    pub(crate) fn get(&self, b: &mut Bencher<'_>, path: &str, expect: u16) {
        let url = self.url(path);
        self.run(b, expect, || self.client.get(&url));
    }

    /// Benchmarks a `GET` against `path` under saturation, asserting the
    /// response status.
    pub(crate) fn get_saturated(&self, b: &mut Bencher<'_>, path: &str, expect: u16) {
        let url = self.url(path);
        self.run_saturated(b, expect, || self.client.get(&url));
    }

    /// Benchmarks whatever request `make` builds, asserting the response status.
    ///
    /// The request is built inside the timed loop on purpose: building it is
    /// part of what the client has to do per request either way, and hoisting it
    /// out would mean rebuilding it from a `RequestBuilder` clone instead.
    pub(crate) fn run<F>(&self, b: &mut Bencher<'_>, expect: u16, make: F)
    where
        F: Fn() -> RequestBuilder,
    {
        self.warmup(&make, expect);
        b.to_async(&self.rt).iter(|| send(make(), expect));
    }

    /// Benchmarks `make` with the profile's concurrency in flight.
    ///
    /// `iters` requests are spread evenly over that many workers, so the
    /// concurrency stays fixed no matter how large a sample criterion asks for.
    /// Each worker is a task of its own, so the client spreads over all of its
    /// threads: driven from a single task, the client tops out long before a
    /// server with several workers does, and the run measures the client.
    pub(crate) fn run_saturated<F>(&self, b: &mut Bencher<'_>, expect: u16, make: F)
    where
        F: Fn() -> RequestBuilder,
    {
        self.warmup(&make, expect);

        let concurrency = self.profile.concurrency;
        // One request per worker to clone from, since a spawned task cannot borrow `make`
        let templates = (0..concurrency).map(|_| make()).collect::<Vec<_>>();
        let templates = &templates;
        b.to_async(&self.rt).iter_custom(move |iters| async move {
            let per_worker = iters / concurrency;
            let remainder = iters % concurrency;

            let start = Instant::now();
            let workers = templates
                .iter()
                .zip(0..concurrency)
                .map(|(template, worker)| {
                    let count = per_worker + u64::from(worker < remainder);
                    let template = template
                        .try_clone()
                        .expect("a request without a streamed body");
                    tokio::spawn(async move {
                        for _ in 0..count {
                            let request = template
                                .try_clone()
                                .expect("a request without a streamed body");
                            send(request, expect).await;
                        }
                    })
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker.await.expect("a benchmark worker panicked");
            }
            start.elapsed()
        });
    }

    /// Opens the pooled connections and warms the code paths before timing.
    fn warmup<F>(&self, make: &F, expect: u16)
    where
        F: Fn() -> RequestBuilder,
    {
        let concurrency = self.profile.concurrency;
        self.rt.block_on(async {
            for _ in 0..(WARMUP_REQUESTS as u64 / concurrency).max(1) {
                join_all((0..concurrency).map(|_| send(make(), expect))).await;
            }
        });
    }
}

/// Sends one request and drains its body, which is what lets the connection go
/// back to the pool. Returns the body length so the caller can black-box it.
async fn send(req: RequestBuilder, expect: u16) -> usize {
    let res = req.send().await.expect("the request failed");
    let status = res.status().as_u16();
    let body = res.bytes().await.expect("reading the response body failed");
    assert_eq!(
        status,
        expect,
        "unexpected status code, body: {}",
        String::from_utf8_lossy(&body)
    );
    body.len()
}

/// Binds an ephemeral port and drives `f` on a dedicated runtime and thread.
///
/// The listener is bound synchronously, so the address is valid and the backlog
/// already accepts connections by the time this returns - no readiness polling
/// and no race with the first request.
fn serve<F, Fut>(workers: usize, f: F) -> SocketAddr
where
    F: FnOnce(TcpListener) -> Fut + Send + 'static,
    Fut: Future<Output = ()>,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind an ephemeral port");
    let addr = listener
        .local_addr()
        .expect("failed to read the local addr");

    thread::spawn(move || {
        Builder::new_multi_thread()
            .worker_threads(workers)
            .enable_all()
            .build()
            .expect("failed to build the server runtime")
            .block_on(f(listener));
    });

    addr
}

fn client(profile: Profile) -> Client {
    let builder = Client::builder()
        .tcp_nodelay(true)
        // Keep the pooled connections alive for the whole run.
        .pool_idle_timeout(None)
        .pool_max_idle_per_host(profile.concurrency as usize);

    #[cfg(all(feature = "http1", not(feature = "http2")))]
    let builder = builder.http1_only();
    #[cfg(feature = "http2")]
    let builder = builder.http2_prior_knowledge();

    builder.build().expect("failed to build the client")
}
