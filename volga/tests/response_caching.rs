#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

use volga::test::TestServer;

#[tokio::test]
async fn it_configures_cache_control_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/testing", |api| {
            api.cache_control(|c| c
                .with_max_age(60)
                .with_immutable()
                .with_public());
            api.map_get("/test", || async { "Pass!" });
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/testing/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_configures_cache_control_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async { "Pass!" })
            .cache_control(|c| c
                .with_max_age(60)
                .with_immutable()
                .with_public());
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_configures_cache_control() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_cache_control(|c| c
                .with_max_age(60)
                .with_immutable()
                .with_public())
        })
        .setup(|app| {
            app.map_get("/test", || async { "Pass!" });
        })
        .build().await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}