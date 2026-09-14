#![allow(missing_docs)]
#![cfg(feature = "test")]

//! Handlers and middleware that return their response directly instead of a future, and
//! synchronous handlers moved to the blocking pool with `blocking`.

use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use volga::error::Error;
use volga::http::Uri;
use volga::test::TestServer;
use volga::{HttpResult, Json, blocking, not_found, ok, status};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct User {
    name: String,
    age: u32,
}

fn greet(name: String) -> String {
    format!("Hello, {name}!")
}

#[tokio::test]
async fn it_maps_a_synchronous_closure() {
    let server = TestServer::spawn(|app| {
        app.map_get("/sum/{x}/{y}", |x: i32, y: i32| x + y);
    })
    .await;

    let response = server
        .client()
        .get(server.url("/sum/2/3"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "5");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_a_synchronous_function() {
    let server = TestServer::spawn(|app| {
        app.map_get("/hello/{name}", greet);
    })
    .await;

    let response = server
        .client()
        .get(server.url("/hello/volga"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Hello, volga!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_the_body_for_a_synchronous_handler() {
    let server = TestServer::spawn(|app| {
        app.map_post("/users", |Json(user): Json<User>| {
            Json(User {
                name: user.name.to_uppercase(),
                age: user.age + 1,
            })
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/users"))
        .json(&User {
            name: "volga".into(),
            age: 1,
        })
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.json::<User>().await.unwrap(),
        User {
            name: "VOLGA".into(),
            age: 2
        }
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_the_error_a_synchronous_handler_returns() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/{id}", |id: String| {
            let id: u32 = id.parse().map_err(Error::client_error)?;
            ok!("user {id}")
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/users/abc"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_both_shapes_side_by_side_in_a_group() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/async", || async { "async" });
            api.map_get("/sync", || "sync");
        });
    })
    .await;

    for shape in ["async", "sync"] {
        let response = server
            .client()
            .get(server.url(&format!("/api/{shape}")))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.text().await.unwrap(), shape);
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_a_blocking_handler() {
    let server = TestServer::spawn(|app| {
        app.map_post(
            "/users",
            blocking(|Json(user): Json<User>| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                format!("{} is {}", user.name, user.age)
            }),
        );
    })
    .await;

    let response = server
        .client()
        .post(server.url("/users"))
        .json(&User {
            name: "volga".into(),
            age: 1,
        })
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "volga is 1");

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_the_error_a_blocking_handler_returns() {
    let server = TestServer::spawn(|app| {
        app.map_get(
            "/fail",
            blocking(|| -> HttpResult { Err(Error::client_error("nope")) }),
        );
    })
    .await;

    let response = server
        .client()
        .get(server.url("/fail"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_a_synchronous_fallback() {
    let server = TestServer::spawn(|app| {
        app.map_fallback(|uri: Uri| not_found!("no route for {uri}"));
    })
    .await;

    let response = server
        .client()
        .get(server.url("/missing"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.text().await.unwrap().contains("/missing"));

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_a_synchronous_error_handler() {
    let server = TestServer::spawn(|app| {
        app.map_err(|error: Error| status!(418, "{}", error));
        app.map_get("/fail", || Err::<(), _>(Error::server_error("boom")));
    })
    .await;

    let response = server
        .client()
        .get(server.url("/fail"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::IM_A_TEAPOT);
    assert!(response.text().await.unwrap().contains("boom"));

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
mod middleware {
    use super::*;
    use volga::headers::{Header, HttpHeaders, headers};
    use volga::{HttpRequestMut, HttpResponse};

    headers! {
        (XTest, "x-test")
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_global_filter() {
        let server = TestServer::spawn(|app| {
            app.filter(|headers: HttpHeaders| headers.get_raw("x-api-key").is_some());
            app.map_get("/test", || "Pass!");
        })
        .await;

        let rejected = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();
        let accepted = server
            .client()
            .get(server.url("/test"))
            .header("x-api-key", "key")
            .send()
            .await
            .unwrap();

        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        assert!(accepted.status().is_success());
        assert_eq!(accepted.text().await.unwrap(), "Pass!");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_route_filter() {
        let server = TestServer::spawn(|app| {
            app.map_get("/test", || "Unreachable!").filter(|| false);
        })
        .await;

        let response = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_filter_returning_a_result() {
        let server = TestServer::spawn(|app| {
            app.map_get("/test/{x}", |x: i32| x)
                .filter(|x: HttpHeaders| {
                    if x.get_raw("x-deny").is_some() {
                        Err(Error::client_error("denied"))
                    } else {
                        Ok(())
                    }
                });
        })
        .await;

        let response = server
            .client()
            .get(server.url("/test/7"))
            .header("x-deny", "1")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_map_ok() {
        let server = TestServer::spawn(|app| {
            app.map_ok(|mut resp: HttpResponse| {
                resp.insert_header(Header::<XTest>::try_from("Test").unwrap());
                resp
            });
            app.map_get("/test", || "Pass!");
        })
        .await;

        let response = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.headers().get("x-test").unwrap(), "Test");
        assert_eq!(response.text().await.unwrap(), "Pass!");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_tap_req() {
        let server = TestServer::spawn(|app| {
            app.tap_req(|mut req: HttpRequestMut| {
                req.insert_header(Header::<XTest>::try_from("Pass!").unwrap());
                req
            });
            app.map_get("/test", |headers: HttpHeaders| {
                let val = headers.try_get::<XTest>()?;
                Ok::<_, Error>(val.to_string())
            });
        })
        .await;

        let response = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.text().await.unwrap(), "x-test: Pass!");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_a_synchronous_map_err_for_a_route() {
        let server = TestServer::spawn(|app| {
            app.map_get("/test", || Err::<(), _>(Error::server_error("Some Error")))
                .map_err(|err: Error| Error::server_error(format!("{err} occurred!")));
        })
        .await;

        let response = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.text().await.unwrap(), "Some Error occurred!");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_adds_synchronous_middleware_to_a_group() {
        let server = TestServer::spawn(|app| {
            app.group("/tests", |api| {
                api.filter(|headers: HttpHeaders| headers.get_raw("x-api-key").is_some());
                api.map_ok(|mut resp: HttpResponse| {
                    resp.insert_header(Header::<XTest>::try_from("Group").unwrap());
                    resp
                });
                api.map_err(|err: Error| Error::server_error(format!("{err} occurred!")));
                api.tap_req(|req: HttpRequestMut| Ok::<_, Error>(req));
                api.map_get("/ok", || "Pass!");
                api.map_get("/fail", || Err::<(), _>(Error::server_error("Some Error")));
            });
        })
        .await;

        let ok = server
            .client()
            .get(server.url("/tests/ok"))
            .header("x-api-key", "key")
            .send()
            .await
            .unwrap();

        assert!(ok.status().is_success());
        assert_eq!(ok.headers().get("x-test").unwrap(), "Group");
        assert_eq!(ok.text().await.unwrap(), "Pass!");

        let fail = server
            .client()
            .get(server.url("/tests/fail"))
            .header("x-api-key", "key")
            .send()
            .await
            .unwrap();

        assert_eq!(fail.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(fail.text().await.unwrap(), "Some Error occurred!");

        server.shutdown().await;
    }
}

#[cfg(feature = "ws")]
mod ws {
    use super::*;
    use volga::ws::{WebSocket, WebSocketConnection, WsEvent};

    #[tokio::test]
    async fn it_maps_a_synchronous_connection_handler() {
        let server = TestServer::spawn(|app| {
            app.map_conn("/ws", |conn: WebSocketConnection| {
                conn.on(|ws: WebSocket| async move {
                    let (mut write, mut read) = ws.split();
                    while let Some(Ok(msg)) = read.recv::<String>().await {
                        match msg {
                            WsEvent::Data(msg) => write.send(msg).await.unwrap(),
                            WsEvent::Close(_frame) => write.close().await.unwrap(),
                            _ => unreachable!(),
                        }
                    }
                })
            });
        })
        .await;

        let mut ws = server.ws("/ws").await;

        ws.send_text("Pass!").await;
        let response = ws.recv_text().await;

        assert_eq!(response, "Pass!");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_maps_a_synchronous_message_handler() {
        let server = TestServer::spawn(|app| {
            app.map_msg("/ws", |msg: String| format!("echo: {msg}"));
        })
        .await;

        let mut ws = server.ws("/ws").await;

        for msg in ["first", "second"] {
            ws.send_text(msg).await;
            assert_eq!(ws.recv_text().await, format!("echo: {msg}"));
        }

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_maps_a_synchronous_json_message_handler_with_an_extractor() {
        let server = TestServer::spawn(|app| {
            app.map_msg("/ws", |Json(user): Json<User>, uri: Uri| {
                format!("{} is {} at {}", user.name, user.age, uri.path())
            });
        })
        .await;

        let mut ws = server.ws("/ws").await;

        ws.send_text(r#"{"name":"volga","age":1}"#).await;

        assert_eq!(ws.recv_text().await, "volga is 1 at /ws");

        server.shutdown().await;
    }
}
