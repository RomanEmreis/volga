use volga::{App, sse};
use futures_util::stream::{repeat_with};
use tokio_stream::StreamExt;
use volga::error::Error;

#[tokio::test]
async fn it_adds_access_control_allow_origin_header() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7944");
        app.map_get("/events", || async {
            let stream = repeat_with(|| "data: Pass!\n\n".into())
                .map(Ok::<_, Error>)
                .take(2);
            sse!(stream)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7944/events")
            .send()
            .await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "data: Pass!\n\ndata: Pass!\n\n")
}