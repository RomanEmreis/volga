#![allow(missing_docs)]
#![cfg(feature = "test")]

use serde::Deserialize;
use std::collections::HashMap;
use volga::error::Error;
use volga::http::endpoints::args::{FromPathArg, PathArg};
use volga::test::TestServer;
use volga::{App, HttpResult, NamedPath, Path, Query, ok};

#[derive(Deserialize)]
struct User {
    name: String,
    age: u32,
}

#[derive(Deserialize)]
struct Repo {
    tenant: String,
    id: u32,
}

/// A path parameter of a type volga does not know about
struct OrderId(u64);

impl FromPathArg for OrderId {
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        arg.parse().map(OrderId)
    }
}

#[tokio::test]
async fn it_reads_route_params() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test/{name}/{age}", |name: String, age: u32| async move {
            format!("My name is: {name}, I'm {age} years old")
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test/John/35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.text().await.unwrap(),
        "My name is: John, I'm 35 years old"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_query_params() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |user: Query<User>| async move {
            format!("My name is: {}, I'm {} years old", user.name, user.age)
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test?name=John&age=35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.text().await.unwrap(),
        "My name is: John, I'm 35 years old"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_query_as_hash_map_params() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/test",
            |query: Query<HashMap<String, String>>| async move {
                let name = query.get("name").unwrap();
                let age = query.get("age").unwrap();

                format!("My name is: {name}, I'm {age} years old")
            },
        );
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test?name=John&age=35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.text().await.unwrap(),
        "My name is: John, I'm 35 years old"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_route_param_declared_by_a_group_prefix() {
    // A group prefix is a route pattern like any other, so it may carry a parameter, and
    // that parameter is bound for every route the group registered - alongside the ones the
    // route declares itself, in the order they appear in the path.
    let server = TestServer::spawn(|app| {
        app.group("/{tenant}", |g| {
            g.map_get("/items/{id}", |tenant: String, id: u32| async move {
                ok!("positional:{tenant}:{id}")
            });
            g.map_get(
                "/pairs/{id}",
                |Path((tenant, id)): Path<(String, u32)>| async move { ok!("path:{tenant}:{id}") },
            );
            g.map_get(
                "/named/{id}",
                |NamedPath(repo): NamedPath<Repo>| async move {
                    ok!("named:{}:{}", repo.tenant, repo.id)
                },
            );
            // The group's own root, where the prefix parameter is all there is
            g.map_get("/", |tenant: String| async move { ok!("root:{tenant}") });
        });
    })
    .await;

    for (path, expected) in [
        ("/acme/items/42", "positional:acme:42"),
        ("/acme/pairs/42", "path:acme:42"),
        ("/acme/named/42", "named:acme:42"),
        ("/acme", "root:acme"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_the_route_params_of_every_group_around_a_route() {
    let server = TestServer::spawn(|app| {
        app.group("/{tenant}", |tenant| {
            tenant.group("/{repo}", |repo| {
                repo.map_get(
                    "/commits/{sha}",
                    |Path((tenant, repo, sha)): Path<(String, String, String)>| async move {
                        ok!("{tenant}/{repo}@{sha}")
                    },
                );
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/acme/volga/commits/b0c85d6"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "acme/volga@b0c85d6");

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_route_param_of_a_type_implementing_from_path_arg() {
    let server = TestServer::spawn(|app| {
        app.map_get("/positional/{id}", |id: OrderId| -> HttpResult {
            ok!("positional:{}", id.0)
        });
        app.map_get("/path/{id}", |Path(id): Path<OrderId>| -> HttpResult {
            ok!("path:{}", id.0)
        });
        app.map_get(
            "/tuple/{id}/{line}",
            |Path((id, line)): Path<(OrderId, u32)>| -> HttpResult { ok!("tuple:{}:{line}", id.0) },
        );
    })
    .await;

    for (path, expected) in [
        ("/positional/42", "positional:42"),
        ("/path/42", "path:42"),
        ("/tuple/42/7", "tuple:42:7"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    for path in ["/positional/nope", "/path/nope", "/tuple/nope/7"] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert_eq!(response.status(), 400, "{path}");
    }

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "uuid")]
async fn it_reads_a_uuid_route_param() {
    use uuid::Uuid;

    let server = TestServer::spawn(|app| {
        app.map_get("/a/{id}", |id: Uuid| -> HttpResult { ok!("{id}") });
        app.map_get("/b/{id}", |Path(id): Path<Uuid>| -> HttpResult {
            ok!("{id}")
        });
        app.map_get("/c/{id}", |Path((id,)): Path<(Uuid,)>| -> HttpResult {
            ok!("{id}")
        });
    })
    .await;

    let id = "0199a0f1-1111-7000-8000-000000000001";

    for prefix in ["/a", "/b", "/c"] {
        let response = server
            .client()
            .get(server.url(&format!("{prefix}/{id}")))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success(), "{prefix}");
        assert_eq!(response.text().await.unwrap(), id, "{prefix}");

        let response = server
            .client()
            .get(server.url(&format!("{prefix}/nope")))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 400, "{prefix}");
    }

    server.shutdown().await;
}

// A handler that reads more path parameters than its route declares would fail every
// request that reaches it, so it is refused where it is mapped (#283)

#[test]
#[should_panic(
    expected = "`GET /sum`: the handler reads 2 path parameters, but the route declares 0"
)]
fn it_panics_where_a_handler_reads_path_params_its_route_does_not_declare() {
    let mut app = App::new();

    app.map_get("/sum", |x: i32, y: i32| x + y);
}

#[test]
#[should_panic(
    expected = "`GET /extra/{id}`: the handler reads 2 path parameters, but the route declares 1"
)]
fn it_panics_where_a_handler_reads_one_path_param_more_than_its_route_declares() {
    let mut app = App::new();

    app.map_get("/extra/{id}", |id: u32, extra: u32| -> HttpResult {
        ok!("{id}:{extra}")
    });
}

#[test]
#[should_panic(
    expected = "`GET /pages/{page}`: the handler reads 2 path parameters, but the route declares 1"
)]
fn it_panics_where_a_path_param_after_an_optional_one_is_not_declared() {
    let mut app = App::new();

    // The optional one still reads the first parameter, which leaves none for the second
    app.map_get(
        "/pages/{page}",
        |page: Option<u32>, size: u32| -> HttpResult { ok!("{page:?}:{size}") },
    );
}

#[test]
#[should_panic(
    expected = "`GET /{tenant}/items`: the handler reads 2 path parameters, but the route declares 1"
)]
fn it_counts_the_path_params_a_group_prefix_declares() {
    let mut app = App::new();

    app.group("/{tenant}", |tenant| {
        tenant.map_get("/items", |tenant: String, id: u32| -> HttpResult {
            ok!("{tenant}:{id}")
        });
    });
}

#[test]
#[should_panic(
    expected = "`POST /pairs/{id}`: the handler reads 2 path parameters, but the route declares 1"
)]
fn it_panics_where_a_path_tuple_is_longer_than_the_params_its_route_declares() {
    let mut app = App::new();

    app.map_post(
        "/pairs/{id}",
        |Path((a, b)): Path<(u32, u32)>| -> HttpResult { ok!("{a}:{b}") },
    );
}

#[test]
#[should_panic(
    expected = "`GET /users/{user_id}/orders/{order_id}`: the handler reads 1 path parameter \
                through `Path<T>`, but the route declares 2"
)]
fn it_panics_where_a_single_type_path_is_mapped_on_several_params() {
    let mut app = App::new();

    // `Path<T>` of a single type does not read the user's id as the order's
    app.map_get(
        "/users/{user_id}/orders/{order_id}",
        |Path(id): Path<OrderId>| -> HttpResult { ok!("{}", id.0) },
    );
}

#[test]
#[should_panic(
    expected = "`GET /orders`: the handler reads 1 path parameter, but the route declares 0"
)]
fn it_panics_where_a_single_type_path_is_mapped_on_no_params() {
    let mut app = App::new();

    app.map_get("/orders", |Path(id): Path<OrderId>| -> HttpResult {
        ok!("{}", id.0)
    });
}

#[test]
#[should_panic(
    expected = "`map_fallback` on group `/api`: the handler reads 1 path parameter, but the \
                group's prefix declares 0"
)]
fn it_panics_where_a_group_fallback_reads_path_params_its_prefix_does_not_declare() {
    let mut app = App::new();

    app.group("/api", |api| {
        api.map_fallback(|Path((id,)): Path<(u32,)>| -> HttpResult { ok!("{id}") });
    });
}

#[test]
#[should_panic(
    expected = "`map_fallback`: the handler reads 1 path parameter, but the application's \
                fallback declares 0"
)]
fn it_panics_where_the_app_fallback_reads_a_path_param() {
    let mut app = App::new();

    app.map_fallback(|Path(id): Path<OrderId>| -> HttpResult { ok!("{}", id.0) });
}

#[tokio::test]
async fn it_maps_a_handler_reading_path_params_its_route_may_leave_out() {
    let server = TestServer::spawn(|app| {
        // `None` where the route has none, so one handler serves both
        let page = |page: Option<u32>| -> HttpResult { ok!("page:{page:?}") };
        app.map_get("/posts", page);
        app.map_get("/posts/{page}", page);

        // The handler is handed the `Err`
        app.map_get("/result", |id: Result<u32, Error>| -> HttpResult {
            ok!("result:{}", id.is_ok())
        });

        // `None`, since `Path<T>` of a single type does not pick one of several
        app.map_get(
            "/users/{user_id}/orders/{order_id}",
            |id: Option<Path<OrderId>>| -> HttpResult { ok!("order:{}", id.is_some()) },
        );

        // A handler may read fewer than the route declares, by position or as a tuple
        app.map_get("/first/{a}/{b}", |a: u32| -> HttpResult {
            ok!("first:{a}")
        });
        app.map_get(
            "/prefix/{a}/{b}",
            |Path((a,)): Path<(u32,)>| -> HttpResult { ok!("prefix:{a}") },
        );

        app.group("/t/{tenant}", |tenant| {
            tenant.map_fallback(|Path(tenant): Path<String>| -> HttpResult {
                ok!("fallback:{tenant}")
            });
        });
    })
    .await;

    for (path, expected) in [
        ("/posts", "page:None"),
        ("/posts/3", "page:Some(3)"),
        ("/result", "result:false"),
        ("/users/7/orders/42", "order:false"),
        ("/first/1/2", "first:1"),
        ("/prefix/1/2", "prefix:1"),
        ("/t/acme", "fallback:acme"),
        ("/t/acme/nope", "fallback:acme"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    server.shutdown().await;
}
