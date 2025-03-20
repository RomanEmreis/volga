use volga::App;
use volga::headers::{
    ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_CREDENTIALS,
    ACCESS_CONTROL_MAX_AGE,
    ACCESS_CONTROL_EXPOSE_HEADERS,
    ORIGIN,
    VARY
};
use volga::http::{Method, StatusCode};

#[tokio::test]
async fn it_adds_access_control_allow_origin_header() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7939")
            .with_cors(|cors| cors.with_origins(["http://127.0.0.1:7939"]));
        app.use_cors();
        app.map_put("/test", || async {});
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.put("http://127.0.0.1:7939/test")
            .header(ORIGIN, "http://127.0.0.1:7939")
            .send()
            .await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1:7939");
}

#[tokio::test]
async fn it_adds_access_control_headers() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7939")
            .with_cors(|cors| cors
                .with_origins(["http://127.0.0.1:7939"])
                .with_methods([Method::PUT])
                .with_any_header());
        app.use_cors();
        app.map_put("/test", || async {});
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.request(Method::OPTIONS, "http://127.0.0.1:7939/test")
            .header(ORIGIN, "http://127.0.0.1:7939")
            .send()
            .await
    }).await.unwrap().unwrap();
    
    assert!(response.status().is_success());
    
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1:7939");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "*");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_METHODS).unwrap(), "PUT");
}