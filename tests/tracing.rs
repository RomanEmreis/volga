use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use volga::{App, Results};

#[tokio::test]
async fn it_adds_request_id() {
    tokio::spawn(async {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .init();
        
        let mut app = App::new()
            .bind("127.0.0.1:7830")
            .with_tracing(|tracing| tracing.with_header());
        app.use_tracing();
        app.map_get("/test", || async {
            Results::text("Pass!")
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7830/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get("request-id").is_some());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}