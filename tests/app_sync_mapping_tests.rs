﻿use volga::{App, Results, SyncEndpointsMapping};

#[tokio::test]
async fn it_maps_to_get_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7879").await?;
        app.map_get("/test", |_req| {
            Results::text("Pass!")
        });
       app.run().await
    });

    let response = tokio::spawn(async {
        reqwest::get("http://127.0.0.1:7879/test").await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_maps_to_post_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7880").await?;
        app.map_post("/test", |_req| {
            Results::text("Pass!")
        });
       app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.post("http://127.0.0.1:7880/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_maps_to_put_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7881").await?;
        app.map_put("/test", |_req| {
            Results::text("Pass!")
        });
       app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.put("http://127.0.0.1:7881/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_maps_to_patch_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7882").await?;
        app.map_patch("/test", |_req| {
            Results::text("Pass!")
        });
       app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.patch("http://127.0.0.1:7882/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_maps_to_delete_request() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7883").await?;
        app.map_delete("/test", |_req| {
            Results::text("Pass!")
        });
       app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.delete("http://127.0.0.1:7883/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}