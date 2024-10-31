﻿use volga::{App, Results, SyncEndpointsMapping, Params};

#[tokio::test]
async fn it_reads_route_params() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7889").await?;

        app.map_get("/test/{name}/{age}", |req| {
            let name = req.param("name")?;
            let age = req.param("age")?;

            let response = format!("My name is: {}, I'm {} years old", name, age);

            Results::text(&response)
        });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.get("http://127.0.0.1:7889/test/John/35").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
}

#[tokio::test]
async fn it_reads_query_params() {
    tokio::spawn(async {
        let mut app = App::build("127.0.0.1:7890").await?;

        app.map_get("/test", |req| {
            let name = req.param("name")?;
            let age = req.param("age")?;

            let response = format!("My name is: {}, I'm {} years old", name, age);

            Results::text(&response)
        });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = reqwest::Client::new();
        client.get("http://127.0.0.1:7890/test?name=John&age=35").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
}