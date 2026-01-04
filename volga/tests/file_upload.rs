#![allow(missing_docs)]
#![cfg(feature = "test")]

use volga::{File, HttpBody, ok};
use volga::test::{TestServer, TempFile};

#[tokio::test]
async fn it_saves_uploaded_file() {
    let temp_file = TempFile::empty();
    let upload_path = temp_file.path;
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

    let temp_file = TempFile::new("Hello, this is some file content!").await;
    let file = tokio::fs::File::open(temp_file.path)
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

    let temp_file = TempFile::empty();
    let upload_path = temp_file.dir_path().to_owned();
    let path_for_handler = upload_path.clone();
    
    let server = TestServer::spawn(move |app| {
        app.map_post("/upload", move |files: Multipart| {
            let path = path_for_handler.clone();
            async move {
                files.save_all(path).await
            }
        });
    }).await;

    let temp_file = TempFile::new("Hello, this is some file content!").await;
    let file_name = temp_file.file_name().to_owned();
    let form = reqwest::multipart::Form::new()
        .file(file_name, temp_file.path.as_path())
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