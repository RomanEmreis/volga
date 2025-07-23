use volga::App;
use volga::app::HostEnv;

#[tokio::test]
async fn it_responds_with_index_file() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7831")
            .set_host_env(HostEnv::new("tests/static"));
        app.use_static_files();
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7831").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");
}

#[tokio::test]
async fn it_responds_with_fallback_file() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7832")
            .with_host_env(|env| env
                .with_content_root("tests/static")
                .with_fallback_file("index.html"));
        app.map_group("/static").use_static_files();
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7832/test/thing").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");
}

#[tokio::test]
async fn it_responds_with_files_listing() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7833")
            .with_host_env(|env| env
                .with_content_root("tests/static")
                .with_files_listing());
        app.use_static_files();
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7833").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html; charset=utf-8");
}