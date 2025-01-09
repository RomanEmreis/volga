use volga::{App, ok};

#[tokio::test]
async fn it_returns_brotli_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7908");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7908/compressed")
            .header("accept-encoding", "br")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_gzip_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7909");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7909/compressed")
            .header("accept-encoding", "gzip")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_deflate_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7910");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7910/compressed")
            .header("accept-encoding", "deflate")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_zstd_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7911");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7911/compressed")
            .header("accept-encoding", "zstd")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_multiple_default_quality_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7912");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7912/compressed")
            .header("accept-encoding", "br, gzip, zstd")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_multiple_different_quality_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7912");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7912/compressed")
            .header("accept-encoding", "br;q=0.9, gzip;q=1, zstd;q=0.8")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_uncompressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7914");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7914/compressed")
            .header("accept-encoding", "identity")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

#[tokio::test]
async fn it_returns_default_brotli_compressed() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7915");
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client
            .get("http://127.0.0.1:7915/compressed")
            .header("accept-encoding", "*")
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
}

fn get_test_data() -> Vec<serde_json::Value> {
    let mut values: Vec<serde_json::Value> = Vec::new();
    for i in 0..10000 {
        values.push(serde_json::json!({ "age": i, "name": i.to_string() }));
    }
    values
}