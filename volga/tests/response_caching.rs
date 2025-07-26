use volga::App;

#[tokio::test]
async fn it_configures_cache_control_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7962");

        app.map_group("/testing")
            .with_cache_control(|c| c
                .with_max_age(60)
                .with_immutable()
                .with_public())
            .map_get("/test", || async { "Pass!" });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7962/testing/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_configures_cache_control_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7963");

        app.map_get("/test", || async { "Pass!" })
            .with_cache_control(|c| c
                .with_max_age(60)
                .with_immutable()
                .with_public());

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7963/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_configures_cache_control() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7964");

        app.use_cache_control(|c| c
            .with_max_age(60)
            .with_immutable()
            .with_public());
        
        app.map_get("/test", || async { "Pass!" });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7964/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "max-age=60, public, immutable");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}