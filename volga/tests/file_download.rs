#![allow(missing_docs)]
#![cfg(feature = "test")]

use volga::file;
use volga::test::{TestServer, TempFile};
use tokio::fs::File;

#[tokio::test]
async fn it_writes_file_response() {
    let temp_file = TempFile::new("Hello, this is some file content!").await;
    let file_path = temp_file.path.clone();
    
    let server = TestServer::spawn(|app| {
        app.map_get("/download", move || {
            let path = file_path.clone();
            async move {
                let file = File::open(&path).await?;
                file!("test_file.txt", file)
            }
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/download"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();

    let content = String::from_utf8_lossy(&response);
    
    assert_eq!(content, "Hello, this is some file content!");
    assert_eq!(content.len(), 33);
    
    server.shutdown().await;
}
