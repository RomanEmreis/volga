#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A route parameter is matched by the position it sits at rather than by what it is
//! called, so every route running through a position shares it - but each endpoint labels
//! the request with the names its own pattern was written with. Two verbs may therefore
//! call one position two things and both be right; what cannot is one verb naming its own
//! route twice, or a `HEAD` disagreeing with the `GET` it answers for (#226).

use serde::Deserialize;
use volga::http::Method;
use volga::test::TestServer;
use volga::{App, NamedPath, ok};

#[derive(Deserialize)]
struct ById {
    id: u32,
}

#[derive(Deserialize)]
struct ByName {
    name: String,
}

#[test]
#[should_panic(
    expected = "ambiguous route `GET /users/{name}`: `GET /users/{id}` is already mapped"
)]
fn it_rejects_a_second_name_for_one_parameter() {
    let mut app = App::new();

    app.map_get("/users/{id}", |id: String| async move { id });
    app.map_get("/users/{name}", |name: String| async move { name });
}

/// A `HEAD` request with no route of its own is answered by the `GET` route, so a `HEAD`
/// mapped by hand describes the same resource and cannot rename what identifies it
#[test]
#[should_panic(
    expected = "ambiguous route `HEAD /users/{name}`: `GET /users/{id}` is already mapped"
)]
fn it_rejects_a_head_named_apart_from_the_get_it_answers_for() {
    let mut app = App::new();

    app.map_get("/users/{id}", |id: String| async move { id });
    app.map_head("/users/{name}", || async { ok!() });
}

#[test]
#[should_panic(
    expected = "ambiguous route `GET /users/{name}`: `GET /users/{id}` is already mapped"
)]
fn it_rejects_a_second_name_across_a_route_group() {
    let mut app = App::new();

    app.group("/users", |users| {
        users.map_get("/{id}", |id: String| async move { id });
    });
    app.map_get("/users/{name}", |name: String| async move { name });
}

/// A group prefix is a route pattern like any other, so a parameter it carries occupies a
/// position the same way one written on a route does
#[test]
#[should_panic(
    expected = "ambiguous route `GET /{id}/items`: `GET /{tenant}/items` is already mapped"
)]
fn it_rejects_a_group_prefix_that_renames_a_parameter() {
    let mut app = App::new();

    app.group("/{tenant}", |tenant| {
        tenant.map_get("/items", || async { ok!() });
    });
    app.map_get("/{id}/items", |id: String| async move { id });
}

/// A route is registered under the name the router reads, so the spelling a conflict is
/// written in does not hide it
#[test]
#[should_panic(
    expected = "ambiguous route `GET /users/{name}`: `GET /users/{id}` is already mapped"
)]
fn it_rejects_a_second_name_written_in_another_spelling() {
    let mut app = App::new();

    app.map_get("/users/{id}/", |id: String| async move { id });
    app.map_get("//users/{name}", |name: String| async move { name });
}

#[test]
#[should_panic(
    expected = "ambiguous route `GET /users/{name}`: `GET /users/{id}` is already mapped"
)]
fn it_rejects_a_second_name_mapped_by_the_generic_map() {
    let mut app = App::new();

    app.map(Method::GET, "/users/{id}", |id: String| async move { id });
    app.map("GET", "/users/{name}", |name: String| async move { name });
}

/// The case this rule exists to leave alone: reading a user by id and creating one by name
/// are two routes that meet at a position without describing one thing, and each binds the
/// name it was written with - including for the extractors that read a parameter by name
#[tokio::test]
async fn it_binds_each_verb_the_name_its_own_route_was_written_with() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}",
            |NamedPath(user): NamedPath<ById>| async move { ok!("read:{}", user.id) },
        );
        app.map_post(
            "/users/{name}",
            |NamedPath(user): NamedPath<ByName>| async move { ok!("created:{}", user.name) },
        );
    })
    .await;

    let read = server
        .client()
        .get(server.url("/users/42"))
        .send()
        .await
        .unwrap();

    assert!(read.status().is_success());
    assert_eq!(read.text().await.unwrap(), "read:42");

    let created = server
        .client()
        .post(server.url("/users/john"))
        .send()
        .await
        .unwrap();

    assert!(created.status().is_success());
    assert_eq!(created.text().await.unwrap(), "created:john");

    server.shutdown().await;
}

/// Two routes on one verb parting at a position never meet at an endpoint either, so they
/// keep their own names too
#[tokio::test]
async fn it_binds_each_route_its_own_name_where_the_two_part() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}/posts",
            |NamedPath(user): NamedPath<ById>| async move { ok!("posts:{}", user.id) },
        );
        app.map_get(
            "/users/{name}/comments",
            |NamedPath(user): NamedPath<ByName>| async move { ok!("comments:{}", user.name) },
        );
    })
    .await;

    for (path, expected) in [
        ("/users/42/posts", "posts:42"),
        ("/users/john/comments", "comments:john"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    server.shutdown().await;
}

/// The same name again is one route gaining a verb, which is how a `HEAD` is written by
/// hand for a dynamic route
#[tokio::test]
async fn it_answers_a_head_mapped_beside_a_get_under_one_name() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/{id}", |id: String| async move { id });
        app.map_head("/users/{id}", || async { ok!([("x-who", "head")]) });
    })
    .await;

    let get = server
        .client()
        .get(server.url("/users/42"))
        .send()
        .await
        .unwrap();

    assert!(get.status().is_success());
    assert_eq!(get.text().await.unwrap(), "42");

    let head = server
        .client()
        .head(server.url("/users/42"))
        .send()
        .await
        .unwrap();

    assert!(head.status().is_success());
    assert_eq!(
        head.headers()
            .get("x-who")
            .map(|value| value.to_str().unwrap()),
        Some("head")
    );

    server.shutdown().await;
}

/// An implicit `HEAD` is the `GET` route answering, so it binds what that route named
#[tokio::test]
async fn it_binds_the_get_route_name_for_an_implicit_head() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}",
            |NamedPath(user): NamedPath<ById>| async move { ok!("read:{}", user.id) },
        );
        app.map_post(
            "/users/{name}",
            |NamedPath(user): NamedPath<ByName>| async move { ok!("created:{}", user.name) },
        );
    })
    .await;

    let head = server
        .client()
        .head(server.url("/users/42"))
        .send()
        .await
        .unwrap();

    assert!(head.status().is_success());

    server.shutdown().await;
}

#[tokio::test]
async fn it_binds_one_parameter_for_every_verb_that_shares_it() {
    let server = TestServer::spawn(|app| {
        for method in [Method::GET, Method::POST, Method::PUT, Method::DELETE] {
            app.map(method.clone(), "/users/{id}", move |id: String| {
                let method = method.clone();
                async move { format!("{method}:{id}") }
            });
        }
    })
    .await;

    for method in [Method::GET, Method::POST, Method::PUT, Method::DELETE] {
        let response = server
            .client()
            .request(method.clone(), server.url("/users/42"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success(), "{method}");
        assert_eq!(response.text().await.unwrap(), format!("{method}:42"));
    }

    server.shutdown().await;
}

/// One name may be written at as many positions as there are routes: what is shared is the
/// position, and these do not share one
#[tokio::test]
async fn it_keeps_parameters_at_different_positions_apart() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}",
            |id: String| async move { format!("user:{id}") },
        );
        app.map_get(
            "/posts/{id}",
            |id: String| async move { format!("post:{id}") },
        );
        app.map_get(
            "/users/{id}/comments/{comment}",
            |id: String, comment: String| async move { format!("comment:{id}:{comment}") },
        );
    })
    .await;

    for (path, expected) in [
        ("/users/42", "user:42"),
        ("/posts/42", "post:42"),
        ("/users/42/comments/7", "comment:42:7"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    server.shutdown().await;
}

/// The way out of a conflict on one verb: a literal segment is matched before the parameter
/// covering it, so the two routes are told apart by the request path instead of by a name
#[tokio::test]
async fn it_tells_two_routes_apart_with_a_literal_segment() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}",
            |id: String| async move { format!("id:{id}") },
        );
        app.map_get("/users/me", || async { "me" });
    })
    .await;

    for (path, expected) in [("/users/me", "me"), ("/users/42", "id:42")] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(response.text().await.unwrap(), expected, "{path}");
    }

    server.shutdown().await;
}

/// Middleware is registered against the pattern of the route it belongs to, so it walks the
/// same positions again and must not read the names it finds there as a second naming
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_accepts_middleware_written_on_a_dynamic_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/{id}", |id: String| async move { id })
            .wrap(|_ctx, _next| async move { volga::status!(403) });
        app.map_post("/users/{name}", |name: String| async move { name })
            .wrap(|_ctx, _next| async move { volga::status!(409) });
    })
    .await;

    let get = server
        .client()
        .get(server.url("/users/42"))
        .send()
        .await
        .unwrap();

    assert_eq!(get.status(), 403);

    let post = server
        .client()
        .post(server.url("/users/john"))
        .send()
        .await
        .unwrap();

    assert_eq!(post.status(), 409);

    server.shutdown().await;
}
