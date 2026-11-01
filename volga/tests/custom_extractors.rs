#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware", feature = "di"))]

//! A type of your own becomes an extractor through `FromRequestHead`, and is taken wherever a
//! built-in extractor of the head is: next to a body extractor, in middleware, in `map_err`.

use serde::Deserialize;
use std::{
    net::TcpListener,
    sync::{Arc, OnceLock},
    time::Duration,
};
use volga::error::Error;
use volga::http::endpoints::args::{FromPathArg, PathArg};
use volga::http::{FromRequestHead, RequestHead, StatusCode};
use volga::test::TestServer;
use volga::{
    App, HttpRequest, HttpRequestMut, HttpResult, Json, NamedPath, Path, Query,
    di::{Dc, Dependencies},
    middleware::Next,
    ok, status,
};

struct OrderId(u64);

impl FromPathArg for OrderId {
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        arg.parse().map(OrderId)
    }
}

/// Reads a header, and refuses a request without it
#[derive(Clone)]
struct CurrentUser(String);

impl FromRequestHead for CurrentUser {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        head.headers()
            .get("x-user")
            .and_then(|user| user.to_str().ok())
            .map(|user| CurrentUser(user.to_owned()))
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "who are you?").into())
    }
}

#[derive(Deserialize)]
struct Tenant {
    tenant: String,
}

/// Reads the path through the head, by name
struct TenantName(String);

impl FromRequestHead for TenantName {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        let path: NamedPath<Tenant> = head.extract()?;
        Ok(TenantName(path.into_inner().tenant))
    }
}

/// Reads another extractor of the user's own through the head
struct Greeting(String);

impl FromRequestHead for Greeting {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        let CurrentUser(user) = head.extract()?;
        Ok(Greeting(format!("hello {user}")))
    }
}

struct Store {
    prefix: &'static str,
}

/// Reads a service from the container, which only `dependencies` can tell `App::run`
struct Labelled(String);

impl FromRequestHead for Labelled {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        let store: Dc<Store> = head.extract()?;
        Ok(Labelled(format!("{}:labelled", store.prefix)))
    }

    fn dependencies(deps: &mut Dependencies) {
        deps.add::<Store>();
    }
}

#[derive(Clone)]
struct Session(Arc<str>);

/// What a middleware awaited: a scoped service, so there is one per request and whatever
/// resolves it during that request reads what the middleware left in it
#[derive(Default)]
struct SessionSlot(OnceLock<Session>);

impl SessionSlot {
    /// Stands in for a round trip to a database
    async fn load(&self, store: &Store, user: &str) {
        tokio::task::yield_now().await;
        let session = Session(format!("{}:{user}", store.prefix).into());
        self.0.set(session).ok();
    }
}

impl FromRequestHead for Session {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        let slot: Dc<SessionSlot> = head.extract()?;
        slot.0
            .get()
            .cloned()
            .ok_or_else(|| Error::server_error("no session"))
    }

    fn dependencies(deps: &mut Dependencies) {
        deps.add::<SessionSlot>();
    }
}

async fn post(server: &TestServer, path: &str, user: Option<&str>, body: &str) -> (u16, String) {
    let mut request = server.client().post(server.url(path)).body(body.to_owned());
    if let Some(user) = user {
        request = request.header("x-user", user);
    }
    let response = request.send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

async fn get(server: &TestServer, path: &str, user: Option<&str>) -> (u16, String) {
    let mut request = server.client().get(server.url(path));
    if let Some(user) = user {
        request = request.header("x-user", user);
    }
    let response = request.send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn it_takes_a_custom_extractor_next_to_a_body_extractor() {
    let server = TestServer::spawn(|app| {
        app.map_post(
            "/orders/{id}",
            async |me: CurrentUser, Path(id): Path<OrderId>, count: Json<u32>| -> HttpResult {
                ok!("{}:{}:{}", me.0, id.0, *count)
            },
        );
    })
    .await;

    assert_eq!(
        post(&server, "/orders/7", Some("ann"), "3").await,
        (200, "ann:7:3".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_with_the_error_of_a_custom_extractor_through_map_err() {
    let server = TestServer::spawn(|app| {
        app.map_err(async |e: Error| status!(e.status().as_u16(), "mapped: {}", e.to_string()));
        app.map_post(
            "/notes",
            async |me: CurrentUser, _: Json<u32>| -> HttpResult { ok!("{}", me.0) },
        );
    })
    .await;

    assert_eq!(
        post(&server, "/notes", None, "3").await,
        (401, "mapped: who are you?".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_takes_a_custom_extractor_next_to_a_positional_param() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/bare/{id}",
            async |id: u64, me: CurrentUser| -> HttpResult { ok!("{}:{id}", me.0) },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/bare/3", Some("ann")).await,
        (200, "ann:3".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_wraps_a_custom_extractor_in_option_and_result() {
    let server = TestServer::spawn(|app| {
        app.map_get("/option", async |me: Option<CurrentUser>| -> HttpResult {
            ok!("{}", me.map_or("anonymous".to_owned(), |me| me.0))
        });
        app.map_get(
            "/result",
            async |me: Result<CurrentUser, Error>| -> HttpResult {
                ok!("{}", me.map_or_else(|e| e.to_string(), |me| me.0))
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/option", None).await,
        (200, "anonymous".to_owned())
    );
    assert_eq!(
        get(&server, "/option", Some("ann")).await,
        (200, "ann".to_owned())
    );
    assert_eq!(
        get(&server, "/result", None).await,
        (200, "who are you?".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_takes_a_custom_extractor_in_filter_and_with() {
    let server = TestServer::spawn(|app| {
        app.map_get("/filter", async || "passed")
            .filter(|me: CurrentUser| me.0 == "ann");
        app.map_get("/with", async || "passed")
            .with(|me: CurrentUser, next: Next| async move {
                let mut response = next.await?;
                response.try_insert_raw_header("x-seen", &me.0)?;
                Ok::<_, Error>(response)
            });
    })
    .await;

    assert_eq!(get(&server, "/filter", Some("ann")).await.0, 200);
    assert_eq!(get(&server, "/filter", Some("bob")).await.0, 400);
    assert_eq!(get(&server, "/filter", None).await.0, 401);

    let response = server
        .client()
        .get(server.url("/with"))
        .header("x-user", "ann")
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["x-seen"], "ann");

    server.shutdown().await;
}

#[tokio::test]
async fn it_takes_an_optional_custom_extractor_in_map_err() {
    let server = TestServer::spawn(|app| {
        app.map_err(async |e: Error, me: Option<CurrentUser>| {
            let user = me.map_or("anonymous".to_owned(), |me| me.0);
            status!(e.status().as_u16(), "{user}: {}", e.to_string())
        });
        app.map_get("/fail", async || -> HttpResult {
            Err(Error::client_error("no"))
        });
    })
    .await;

    assert_eq!(
        get(&server, "/fail", Some("ann")).await,
        (400, "ann: no".to_owned())
    );
    assert_eq!(
        get(&server, "/fail", None).await,
        (400, "anonymous: no".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_custom_extractor_through_the_head_of_another() {
    let server = TestServer::spawn(|app| {
        app.map_get("/greeting", async |greeting: Greeting| -> HttpResult {
            ok!("{}", greeting.0)
        });
        app.map_get("/filtered", async || "passed")
            .filter(|greeting: Greeting| greeting.0 == "hello ann");
    })
    .await;

    assert_eq!(
        get(&server, "/greeting", Some("ann")).await,
        (200, "hello ann".to_owned())
    );
    assert_eq!(get(&server, "/greeting", None).await.0, 401);
    assert_eq!(get(&server, "/filtered", Some("ann")).await.0, 200);

    server.shutdown().await;
}

#[tokio::test]
async fn it_leaves_the_path_params_for_a_custom_extractor_next_to_a_path_extractor() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/tenants/{tenant}/alone",
            async |tenant: TenantName| -> HttpResult { ok!("{}", tenant.0) },
        );
        app.map_get(
            "/tenants/{tenant}/orders/{id}",
            async |Path((_, id)): Path<(String, u64)>, tenant: TenantName| -> HttpResult {
                ok!("{}:{id}", tenant.0)
            },
        );
        app.map_get(
            "/tenants/{tenant}/positional/{id}",
            async |tenant: TenantName, name: String, id: u64| -> HttpResult {
                ok!("{}:{name}:{id}", tenant.0)
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/tenants/acme/alone", None).await,
        (200, "acme".to_owned())
    );
    assert_eq!(
        get(&server, "/tenants/acme/orders/7", None).await,
        (200, "acme:7".to_owned())
    );
    assert_eq!(
        get(&server, "/tenants/acme/positional/7", None).await,
        (200, "acme:acme:7".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_service_through_the_head() {
    let server = TestServer::spawn(|app| {
        app.add_singleton(Store { prefix: "store" });
        app.map_get("/labelled", async |label: Labelled| -> HttpResult {
            ok!("{}", label.0)
        });
    })
    .await;

    assert_eq!(
        get(&server, "/labelled", None).await,
        (200, "store:labelled".to_owned())
    );

    server.shutdown().await;
}

async fn startup_error(app: App) -> std::io::Error {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    tokio::time::timeout(Duration::from_secs(5), app.run_with_std_listener(listener))
        .await
        .expect("the app started and kept serving")
        .expect_err("the app started")
}

#[tokio::test]
async fn it_refuses_to_start_on_a_handler_taking_a_custom_extractor_of_a_missing_service() {
    let mut app = App::new().without_greeter();
    app.map_get("/labelled", async |_: Labelled| "unreachable");

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(std::any::type_name::<Store>()),
        "{message}"
    );
}

#[tokio::test]
async fn it_refuses_to_start_on_a_filter_taking_a_custom_extractor_of_a_missing_service() {
    let mut app = App::new().without_greeter();
    app.map_get("/labelled", async || "unreachable")
        .filter(|_: Labelled| true);

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(std::any::type_name::<Store>()),
        "{message}"
    );
}

#[tokio::test]
async fn it_hands_what_a_middleware_awaited_to_a_custom_extractor() {
    let server = TestServer::spawn(|app| {
        app.add_singleton(Store { prefix: "store" });
        app.add_scoped_default::<SessionSlot>();

        app.group("/tap", |api| {
            api.tap_req(
                async |req: HttpRequestMut, store: Dc<Store>, slot: Dc<SessionSlot>| {
                    let CurrentUser(user) = req.extract()?;
                    slot.load(&store, &user).await;
                    Ok::<_, Error>(req)
                },
            );
            api.map_post("/chat", async |me: Session, _: Json<u32>| -> HttpResult {
                ok!("{}", me.0)
            });
        });

        app.group("/with", |api| {
            api.with(
                |CurrentUser(user): CurrentUser,
                 store: Dc<Store>,
                 slot: Dc<SessionSlot>,
                 next: Next| async move {
                    slot.load(&store, &user).await;
                    next.await
                },
            );
            api.map_post("/chat", async |me: Session, _: Json<u32>| -> HttpResult {
                ok!("{}", me.0)
            });
        });
    })
    .await;

    for path in ["/tap/chat", "/with/chat"] {
        // One slot per request: the second user does not read what the first one left
        for user in ["ann", "bob"] {
            assert_eq!(
                post(&server, path, Some(user), "3").await,
                (200, format!("store:{user}")),
                "{path}"
            );
        }
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_refuses_to_start_on_a_custom_extractor_of_a_slot_nobody_registered() {
    let mut app = App::new().without_greeter();
    app.map_get("/me", async |_: Session| "unreachable");

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(std::any::type_name::<SessionSlot>()),
        "{message}"
    );
}

#[tokio::test]
async fn it_takes_a_custom_extractor_before_a_request() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/me",
            async |me: CurrentUser, req: HttpRequest| -> HttpResult {
                ok!("{}:{}", me.0, req.uri().path())
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/me", Some("ann")).await,
        (200, "ann:/me".to_owned())
    );

    server.shutdown().await;
}

/// What the head looks like in a log line
struct HeadDebug(String);

impl FromRequestHead for HeadDebug {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        Ok(HeadDebug(format!("{head:?}")))
    }
}

#[tokio::test]
async fn it_keeps_the_headers_out_of_the_debug_output_of_the_head() {
    let server = TestServer::spawn(|app| {
        app.map_get("/debug", async |head: HeadDebug| -> HttpResult {
            ok!("{}", head.0)
        });
    })
    .await;

    assert_eq!(
        get(&server, "/debug", Some("ann")).await,
        (200, "RequestHead(..)".to_owned())
    );

    server.shutdown().await;
}

#[derive(Deserialize)]
struct Page {
    size: u32,
}

/// The built-in extractors of the head are read through it, in a handler and in middleware
struct PageSize(u32);

impl FromRequestHead for PageSize {
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        let page: Query<Page> = head.extract()?;
        Ok(PageSize(page.into_inner().size))
    }
}

#[tokio::test]
async fn it_reads_a_built_in_extractor_through_the_head() {
    let server = TestServer::spawn(|app| {
        app.map_get("/handler", async |page: PageSize| -> HttpResult {
            ok!("{}", page.0)
        });
        app.map_get("/filter", async || "passed")
            .filter(|page: PageSize| page.0 == 10);
    })
    .await;

    assert_eq!(
        get(&server, "/handler?size=10", None).await,
        (200, "10".to_owned())
    );
    assert_eq!(get(&server, "/filter?size=10", None).await.0, 200);
    assert_eq!(get(&server, "/filter?size=9", None).await.0, 400);

    server.shutdown().await;
}
