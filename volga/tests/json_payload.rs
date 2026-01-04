#![allow(missing_docs)]
#![cfg(feature = "test")]

use serde::{Deserialize, Serialize};
use volga::{ok, Results, Json};
use volga::test::TestServer;

#[derive(Deserialize, Serialize)]
struct User {
    name: String,
    age: u32
}

#[tokio::test]
async fn it_reads_json_payload() {
    let server = TestServer::spawn(|app| {
        app.map_post("/test", |user: Json<User>| async move {
            let response = format!("My name is: {}, I'm {} years old", user.name, user.age);
            
            Results::text(&response)
        });
    }).await;

    let user = User { name: String::from("John"), age: 35 };
    
    let response = server.client()
        .post(server.url("/test"))
        .json(&user)
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "My name is: John, I'm 35 years old");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_writes_json_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async move {
            let user = User { name: String::from("John"), age: 35 };
            
            Results::json(&user)
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap()
        .json::<User>()
        .await
        .unwrap();

    assert_eq!(response.name, "John");
    assert_eq!(response.age, 35);
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_writes_json_using_macro_response() {
    let server = TestServer::spawn(|app| { 
        app.map_get("/test", || async move {
            let user = User { name: String::from("John"), age: 35 };
            ok!(user)
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap()
        .json::<User>()
        .await
        .unwrap();

    assert_eq!(response.name, "John");
    assert_eq!(response.age, 35);
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_writes_untyped_json_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async move {
            ok!({ "name": "John", "age": 35 })
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap()
        .json::<User>()
        .await
        .unwrap();

    assert_eq!(response.name, "John");
    assert_eq!(response.age, 35);
    
    server.shutdown().await;
}