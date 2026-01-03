#![allow(missing_docs)]

use volga::{File, HttpBody, ok};
mod common;
use common::TestServer;

#[tokio::test]
async fn it_saves_uploaded_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let upload_path = temp_dir.path().join("uploaded.txt");
    let path_for_handler = upload_path.clone();

    let server = TestServer::spawn(move |app| {
        app.map_post("/upload", move |file: File| {
            let path = path_for_handler.clone();
            async move {
                file.save_as(&path).await?;
                ok!()
            }
        });
    }).await;

    let file = tokio::fs::File::open("tests/resources/test_file.txt")
        .await
        .unwrap();
    let body = HttpBody::file(file);

    let response = server.client()
        .post(server.url("/upload"))
        .body(reqwest::Body::wrap(body))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(upload_path.exists());

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "multipart")]
async fn it_saves_uploaded_multipart() {
    use volga::Multipart;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let upload_path = temp_dir.path().to_path_buf();
    let path_for_handler = upload_path.clone();
    
    let server = TestServer::spawn(move |app| {
        app.map_post("/upload", move |files: Multipart| {
            let path = path_for_handler.clone();
            async move {
                files.save_all(path).await
            }
        });
    }).await;

    let form = reqwest::multipart::Form::new()
        .file("test_file", "tests/resources/test_file.txt")
        .await
        .unwrap();

    let response = server.client()
        .post(server.url("/upload"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(upload_path.exists());

    server.shutdown().await;
}