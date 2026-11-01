#![allow(missing_docs)]
#![cfg(feature = "test")]

//! `HttpRequest` is handed the whole request, so it takes no other extractor of the body and
//! stands last. What stands before it reads the request without taking it apart.

use http_body_util::BodyExt;
use std::collections::HashMap;
use volga::http::{Method, Uri};
use volga::test::TestServer;
use volga::{HttpRequest, HttpResult, Path, Query, ok};

type Params = Query<HashMap<String, String>>;

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let response = server.client().get(server.url(path)).send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn it_reads_a_request_after_extractors_of_the_head() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/head",
            async |uri: Uri, q: Params, method: Method, req: HttpRequest| -> HttpResult {
                ok!(
                    "{method}:{}:{}:{}",
                    q.get("a").unwrap(),
                    uri.path(),
                    req.uri().path()
                )
            },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/head?a=1").await,
        (200, "GET:1:/head:/head".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_request_after_path_params_and_leaves_them_in_it() {
    fn args(req: &HttpRequest) -> String {
        let args: Vec<_> = req.path_args().map(|(k, v)| format!("{k}={v}")).collect();
        args.join(",")
    }

    let server = TestServer::spawn(|app| {
        app.map_get(
            "/path/{id}",
            async |Path(id): Path<u32>, req: HttpRequest| -> HttpResult {
                ok!("{id}:{}", args(&req))
            },
        );
        app.map_get(
            "/positional/{id}",
            async |id: u32, req: HttpRequest| -> HttpResult { ok!("{id}:{}", args(&req)) },
        );
        app.map_get(
            "/owned/{name}",
            async |name: String, req: HttpRequest| -> HttpResult { ok!("{name}:{}", args(&req)) },
        );
    })
    .await;

    assert_eq!(get(&server, "/path/7").await, (200, "7:id=7".to_owned()));
    assert_eq!(
        get(&server, "/positional/7").await,
        (200, "7:id=7".to_owned())
    );
    assert_eq!(
        get(&server, "/owned/ann").await,
        (200, "ann:name=ann".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_keeps_the_query_cache_of_a_request_handed_to_a_handler() {
    let server = TestServer::spawn(|app| {
        app.map_get("/query", async |uri: Uri, req: HttpRequest| -> HttpResult {
            let args: Vec<_> = req.query_args().map(|(k, v)| format!("{k}={v}")).collect();
            ok!("{}:{}", uri.path(), args.join(","))
        });
    })
    .await;

    assert_eq!(
        get(&server, "/query?a=1&b=2").await,
        (200, "/query:a=1,b=2".to_owned())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_leaves_the_path_params_in_a_request_taken_alone() {
    let server = TestServer::spawn(|app| {
        app.map_get("/alone/{id}", async |req: HttpRequest| -> HttpResult {
            let id: Path<u32> = req.extract()?;
            ok!("{}", id.into_inner())
        });
    })
    .await;

    assert_eq!(get(&server, "/alone/7").await, (200, "7".to_owned()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_leaves_the_body_in_a_request_after_extractors_of_the_head() {
    let server = TestServer::spawn(|app| {
        app.map_post("/echo", async |uri: Uri, req: HttpRequest| -> HttpResult {
            let body = req.into_body().collect().await?.to_bytes();
            ok!("{}:{}", uri.path(), String::from_utf8_lossy(&body))
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/echo"))
        .body("hello")
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "/echo:hello");

    server.shutdown().await;
}

/// The signature neva's routes have
#[tokio::test]
#[cfg(feature = "di")]
async fn it_reads_a_request_after_a_service() {
    use volga::di::Dc;

    struct Store(&'static str);

    async fn route(store: Dc<Store>, req: HttpRequest) -> HttpResult {
        ok!("{}:{}", store.0, req.uri().path())
    }

    let server = TestServer::spawn(|app| {
        app.add_singleton(Store("store"));
        app.map_get("/route", route);
    })
    .await;

    assert_eq!(
        get(&server, "/route").await,
        (200, "store:/route".to_owned())
    );

    server.shutdown().await;
}
