#![allow(missing_docs)]
#![cfg(feature = "test")]

use serde::Deserialize;
use std::collections::HashMap;
use volga::error::Error;
use volga::http::endpoints::args::{FromPathArg, PathArg};
use volga::test::TestServer;
use volga::{HttpResult, NamedPath, Path, Query, ok};

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
