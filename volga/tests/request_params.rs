#![allow(missing_docs)]
#![cfg(feature = "test")]

use std::collections::HashMap;
use serde::Deserialize;
use volga::{Results, Query};
use volga::test::TestServer;

#[derive(Deserialize)]
struct User {
    name: String,
    age: u32
}

#[tokio::test]
async fn it_reads_route_params() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test/{name}/{age}", |name: String, age: u32| async move {
            let response = format!("My name is: {name}, I'm {age} years old");
            Results::text(&response)
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test/John/35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_query_params() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |user: Query<User>| async move {
            let response = format!("My name is: {}, I'm {} years old", user.name, user.age);
            Results::text(&response)
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test?name=John&age=35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_query_as_hash_map_params() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |query: Query<HashMap<String, String>>| async move {
            let name = query.get("name").unwrap();
            let age = query.get("age").unwrap();
            let response = format!("My name is: {name}, I'm {age} years old");
            Results::text(&response)
        });
    }).await;

    let response = server.client()
        .get(server.url("/test?name=John&age=35"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
    
    server.shutdown().await;
}