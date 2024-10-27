﻿use volga::{App, AsyncMiddlewareMapping, Results, SyncEndpointsMapping};

#[tokio::test]
async fn it_adds_middleware_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7884").await?;

        app.use_middleware(|context, next| async move {
            next(context).await
        });
        app.use_middleware(|_, _| async move {
            Results::text("Pass!")
        });

        app.map_get("/test", |_req| {
            Results::text("Unreachable!")
        });

       app.run().await
    });

    let response = tokio::spawn(async {
        reqwest::get("http://127.0.0.1:7884/test").await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}