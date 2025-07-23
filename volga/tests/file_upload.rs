use volga::{App, File, HttpBody, ok};

#[tokio::test]
async fn it_saves_uploaded_file() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7899");

        app.map_post("/upload", |file: File| async move {
            file.save_as("tests/resources/test_file_saved.txt").await?;
            ok!()
        });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        let file = tokio::fs::File::open("tests/resources/test_file.txt").await.unwrap();
        let body = HttpBody::file(file);
        
        client.post("http://127.0.0.1:7899/upload").body(reqwest::Body::wrap(body)).send().await.unwrap()
    }).await.unwrap();


    assert!(response.status().is_success());
}

#[tokio::test]
#[cfg(feature = "multipart")]
async fn it_saves_uploaded_multipart() {
    use volga::Multipart;
    
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7938");

        app.map_post("/upload", |files: Multipart| async move {
            files.save_all("tests/resources").await
        });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        
        let form = reqwest::multipart::Form::new()
            .file("test_file", "tests/resources/test_file.txt")
            .await
            .unwrap();
        
        client.post("http://127.0.0.1:7938/upload")
            .multipart(form)
            .send()
            .await
            .unwrap()
    }).await.unwrap();


    assert!(response.status().is_success());
}