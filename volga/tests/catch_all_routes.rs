#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A catch-all parameter, `{*name}`, binds the rest of the path - at least one segment of
//! it - as one value (#227). At every position a literal is read first, a parameter second
//! and a catch-all last, and the first position two routes differ at decides between them.

use serde::Deserialize;
use volga::test::TestServer;
use volga::{App, NamedPath, Path, ok};

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let response = server
        .client()
        .get(server.url(path))
        .send()
        .await
        .expect("the request failed");
    let status = response.status().as_u16();
    (status, response.text().await.expect("no body"))
}

#[tokio::test]
async fn it_binds_the_rest_of_the_path() {
    let server = TestServer::spawn(|app| {
        app.map_get("/files/{*path}", |path: String| async move { path });
    })
    .await;

    assert_eq!(get(&server, "/files/a").await, (200, "a".into()));
    assert_eq!(
        get(&server, "/files/a/b/c.txt").await,
        (200, "a/b/c.txt".into())
    );
    assert_eq!(get(&server, "/files/a/b/").await, (200, "a/b/".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_does_not_answer_an_empty_tail() {
    let server = TestServer::spawn(|app| {
        app.map_get("/files/{*path}", |path: String| async move { path });
    })
    .await;

    assert_eq!(get(&server, "/files").await.0, 404);
    assert_eq!(get(&server, "/files/").await.0, 404);

    server.shutdown().await;
}

#[tokio::test]
async fn it_binds_the_parameters_before_a_catch_all() {
    #[derive(Deserialize)]
    struct Params {
        id: u32,
        path: String,
    }

    let server = TestServer::spawn(|app| {
        app.map_get(
            "/users/{id}/files/{*path}",
            |Path((id, path)): Path<(u32, String)>| async move { ok!("{id}:{path}") },
        );
        app.map_get(
            "/named/{id}/files/{*path}",
            |NamedPath(params): NamedPath<Params>| async move {
                ok!("{}:{}", params.id, params.path)
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/users/7/files/a/b").await,
        (200, "7:a/b".into())
    );
    assert_eq!(
        get(&server, "/named/7/files/a/b").await,
        (200, "7:a/b".into())
    );

    server.shutdown().await;
}

/// A tail is read by name as a whole, separators and all: an `&` in it does not end it and
/// start a field the route never bound
#[tokio::test]
async fn it_reads_a_tail_carrying_a_pair_separator_by_name() {
    #[derive(Deserialize)]
    struct Params {
        path: String,
        admin: Option<bool>,
    }

    let server = TestServer::spawn(|app| {
        app.map_get(
            "/files/{*path}",
            |NamedPath(params): NamedPath<Params>| async move {
                ok!("{}:{:?}", params.path, params.admin)
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/files/a&admin=true/b").await,
        (200, "a&admin=true/b:None".into())
    );

    server.shutdown().await;
}

/// `/{lang}/{page}` would read `/assets/app.js` too, but the two routes differ first at the
/// position where one of them has a literal
#[tokio::test]
async fn it_decides_at_the_first_position_routes_differ_in_any_registration_order() {
    let server = TestServer::spawn(|app| {
        app.map_get("/{lang}/{page}", |lang: String, page: String| async move {
            ok!("page:{lang}:{page}")
        });
        app.map_get("/assets/{*path}", |path: String| async move {
            ok!("asset:{path}")
        });
    })
    .await;

    assert_eq!(
        get(&server, "/assets/app.js").await,
        (200, "asset:app.js".into())
    );
    assert_eq!(
        get(&server, "/assets/css/app.css").await,
        (200, "asset:css/app.css".into())
    );
    assert_eq!(get(&server, "/en/home").await, (200, "page:en:home".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_what_no_other_route_does_with_a_root_catch_all() {
    let server = TestServer::spawn(|app| {
        app.map_get("/api/users/{id}", |id: u32| async move { ok!("user:{id}") });
        app.map_get(
            "/{*path}",
            |path: String| async move { ok!("shell:{path}") },
        );
    })
    .await;

    assert_eq!(get(&server, "/api/users/1").await, (200, "user:1".into()));
    assert_eq!(
        get(&server, "/api/users/1/extra").await,
        (200, "shell:api/users/1/extra".into())
    );
    assert_eq!(
        get(&server, "/dashboard/settings").await,
        (200, "shell:dashboard/settings".into())
    );
    assert_eq!(get(&server, "/").await.0, 404);

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_a_method_it_is_not_mapped_for_with_405() {
    let server = TestServer::spawn(|app| {
        app.map_get("/files/{*path}", |path: String| async move { path });
        app.map_put("/files/{*path}", |path: String| async move { path });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/files/a/b"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 405);
    assert_eq!(response.headers().get("allow").unwrap(), "GET,PUT,HEAD");

    let head = server
        .client()
        .head(server.url("/files/a/b"))
        .send()
        .await
        .unwrap();

    assert!(head.status().is_success());

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_a_catch_all_inside_a_group() {
    let server = TestServer::spawn(|app| {
        app.group("/static", |group| {
            group.map_get("/{*path}", |path: String| async move { path });
        });
    })
    .await;

    assert_eq!(get(&server, "/static/a/b").await, (200, "a/b".into()));
    assert_eq!(get(&server, "/static").await.0, 404);

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "middleware")]
async fn it_runs_the_middleware_bound_to_a_catch_all_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/files/{*path}", |path: String| async move { path })
            .filter(|Path((path,)): Path<(String,)>| !path.starts_with("private/"));

        app.group("/api", |api| {
            api.filter(|| false);
            api.map_get("/{*rest}", |rest: String| async move { rest });
        });
    })
    .await;

    assert_eq!(get(&server, "/files/a/b").await, (200, "a/b".into()));
    assert!(get(&server, "/files/private/key").await.0 >= 400);
    assert!(get(&server, "/api/anything/here").await.0 >= 400);

    server.shutdown().await;
}

#[test]
#[should_panic(
    expected = "invalid route `/{*path}/edit`: a catch-all parameter reads the rest of the path"
)]
fn it_rejects_a_segment_after_a_catch_all() {
    let mut app = App::new();

    app.map_get("/{*path}/edit", |path: String| async move { path });
}

/// A group prefix is a route pattern like any other, so a catch-all in it has the group's
/// routes following it
#[test]
#[should_panic(
    expected = "invalid route `/files/{*path}/edit`: a catch-all parameter reads the rest of the path"
)]
fn it_rejects_a_route_under_a_group_prefix_ending_in_a_catch_all() {
    let mut app = App::new();

    app.group("/files/{*path}", |group| {
        group.map_get("/edit", |path: String| async move { path });
    });
}

#[test]
#[should_panic(
    expected = "ambiguous route `GET /files/{*rest}`: `GET /files/{*path}` is already mapped"
)]
fn it_rejects_a_second_name_for_one_catch_all() {
    let mut app = App::new();

    app.map_get("/files/{*path}", |path: String| async move { path });
    app.map_get("/files/{*rest}", |rest: String| async move { rest });
}
