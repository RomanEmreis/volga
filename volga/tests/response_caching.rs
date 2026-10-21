#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

use volga::{headers::CacheControl, test::TestServer};

#[tokio::test]
async fn it_configures_cache_control_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/testing", |api| {
            api.with_cache_control(|c| c.with_max_age(60).with_immutable().with_public());
            api.map_get("/test", || async { "Pass!" });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/testing/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("Cache-Control").unwrap(),
        "max-age=60, public, immutable"
    );
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_configures_cache_control_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async { "Pass!" })
            .with_cache_control(|c| c.with_max_age(60).with_immutable().with_public());
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("Cache-Control").unwrap(),
        "max-age=60, public, immutable"
    );
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_configures_cache_control() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_cache_control(|c| c.with_max_age(60).with_immutable().with_public())
        })
        .setup(|app| {
            app.map_get("/test", || async { "Pass!" });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("Cache-Control").unwrap(),
        "max-age=60, public, immutable"
    );
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_sets_cache_control() {
    let server = TestServer::builder()
        .configure(|app| {
            app.set_cache_control(
                CacheControl::default()
                    .with_max_age(60)
                    .with_immutable()
                    .with_public(),
            )
        })
        .setup(|app| {
            app.map_get("/test", || async { "Pass!" });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("Cache-Control").unwrap(),
        "max-age=60, public, immutable"
    );

    server.shutdown().await;
}

#[tokio::test]
#[allow(deprecated)]
async fn it_configures_cache_control_under_the_old_name() {
    let server = TestServer::spawn(|app| {
        app.group("/testing", |api| {
            api.cache_control(|c| c.with_max_age(60).with_public());
            api.map_get("/test", || async { "Pass!" });
        });
        app.map_get("/test", || async { "Pass!" })
            .cache_control(|c| c.with_max_age(60).with_immutable().with_public());
    })
    .await;

    for (path, expected) in [
        ("/testing/test", "max-age=60, public"),
        ("/test", "max-age=60, public, immutable"),
    ] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert_eq!(
            response.headers().get("Cache-Control").unwrap(),
            expected,
            "{path}"
        );
    }

    server.shutdown().await;
}
