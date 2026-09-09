#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A literal segment mapped on the way to a longer route must not hide the parameter
//! sitting at the same position: `GET /a/{b}` keeps answering `/a/b` even once some
//! unrelated route maps `/a/b/c/d`.

use volga::test::TestServer;

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
async fn it_reads_a_parameter_where_the_literal_branch_ends_short() {
    let server = TestServer::spawn(|app| {
        app.map_get("/a/b/c/d/e/i/k", || async { "deep" });
        app.map_get("/a/{b}", |b: String| async move { format!("param:{b}") });
    })
    .await;

    assert_eq!(get(&server, "/a/b").await, (200, "param:b".into()));
    assert_eq!(get(&server, "/a/zz").await, (200, "param:zz".into()));
    assert_eq!(get(&server, "/a/b/c/d/e/i/k").await, (200, "deep".into()));
    assert_eq!(get(&server, "/a/b/c").await.0, 404);

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_parameter_where_the_literal_branch_parts_deeper() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/me/settings", || async { "settings" });
        app.map_get("/users/{id}/posts", |id: String| async move {
            format!("posts:{id}")
        });
    })
    .await;

    assert_eq!(
        get(&server, "/users/me/posts").await,
        (200, "posts:me".into())
    );
    assert_eq!(
        get(&server, "/users/42/posts").await,
        (200, "posts:42".into())
    );
    assert_eq!(
        get(&server, "/users/me/settings").await,
        (200, "settings".into())
    );
    assert_eq!(get(&server, "/users/me/unmapped").await.0, 404);

    server.shutdown().await;
}

#[tokio::test]
async fn it_prefers_the_literal_that_leads_somewhere() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/me", || async { "me" });
        app.map_get(
            "/users/{id}",
            |id: String| async move { format!("id:{id}") },
        );
    })
    .await;

    assert_eq!(get(&server, "/users/me").await, (200, "me".into()));
    assert_eq!(get(&server, "/users/42").await, (200, "id:42".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_keeps_the_method_mismatch_of_a_literal_that_matched() {
    let server = TestServer::spawn(|app| {
        app.map_post("/users/me", || async { "me" });
        app.map_get(
            "/users/{id}",
            |id: String| async move { format!("id:{id}") },
        );
    })
    .await;

    // `/users/me` is mapped, just not for GET. Reading it as `{id}` instead would
    // answer a request that belongs to the literal route.
    assert_eq!(get(&server, "/users/me").await.0, 405);
    assert_eq!(get(&server, "/users/42").await, (200, "id:42".into()));

    server.shutdown().await;
}
