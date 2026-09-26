//! The `Err` of a handler's `Result<T, E>` goes to the error handler as an `Error`, through
//! `IntoError`, whatever `E` is.

#![allow(missing_docs)]
#![cfg(feature = "test")]

use serde::Serialize;
use volga::{
    HttpResult, Json,
    error::{Error, IntoError},
    http::StatusCode,
    ok, status,
    test::TestServer,
};

/// Status, `Content-Type` and body of a `GET`
async fn get(server: &TestServer, path: &str) -> (u16, String, String) {
    let res = server.client().get(server.url(path)).send().await.unwrap();
    let status = res.status().as_u16();
    let content_type = res
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (status, content_type, res.text().await.unwrap())
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
}

enum ApiError {
    NotFound(u64),
    Conflict,
}

impl IntoError for ApiError {
    fn into_error(self) -> Error {
        match self {
            ApiError::NotFound(id) => {
                Error::from_parts(StatusCode::NOT_FOUND, None, format!("item {id} not found"))
                    .with_response(Json(ErrorBody { code: "not_found" }))
            }
            ApiError::Conflict => Error::from_parts(StatusCode::CONFLICT, None, "conflict"),
        }
    }
}

struct Gone;

/// `IntoError` alone: it is a handler's `Err`, and `?` converts it into an `Error`
impl IntoError for Gone {
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::GONE, None, "gone")
    }
}

#[tokio::test]
async fn it_hands_every_err_to_the_error_handler() {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_err(|e: Error| async move { status!(e.status().as_u16(), "handled: {e}") });
            app.map_get("/string", || Err::<&'static str, _>(String::from("boom")));
            app.map_get("/str", || Err::<&'static str, _>("boom"));
            app.map_get("/status", || Err::<&'static str, _>(StatusCode::NOT_FOUND));
            app.map_get("/tuple", || {
                Err::<&'static str, _>((StatusCode::BAD_REQUEST, "name is required"))
            });
            app.map_get("/own", || Err::<&'static str, _>(ApiError::Conflict));
            app.map_get("/gone", || Err::<&'static str, _>(Gone));
            app.map_get("/question-mark", || -> HttpResult {
                Err::<(), _>(Gone)?;
                ok!()
            });
            app.map_get("/io", || {
                Err::<&'static str, _>(std::io::Error::new(std::io::ErrorKind::NotFound, "no file"))
            });
        })
        .build()
        .await;

    let cases = [
        ("/string", 500, "handled: boom"),
        ("/str", 500, "handled: boom"),
        ("/status", 404, "handled: Not Found"),
        ("/tuple", 400, "handled: name is required"),
        ("/own", 409, "handled: conflict"),
        ("/gone", 410, "handled: gone"),
        ("/question-mark", 410, "handled: gone"),
        ("/io", 404, "handled: no file"),
    ];

    for (path, status, body) in cases {
        let (actual_status, _, actual_body) = get(&server, path).await;
        assert_eq!(
            (actual_status, actual_body.as_str()),
            (status, body),
            "{path}"
        );
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_an_err_with_the_default_error_handler() {
    let server = TestServer::spawn(|app| {
        app.map_get("/string", || Err::<&'static str, _>("boom"));
        app.map_get("/ok", || Ok::<_, String>("fine"));
    })
    .await;

    assert_eq!(
        get(&server, "/string").await,
        (500, "text/plain; charset=utf-8".into(), "boom".into())
    );
    assert_eq!(
        get(&server, "/ok").await,
        (200, "text/plain; charset=utf-8".into(), "fine".into())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_with_the_response_an_error_carries() {
    let server = TestServer::spawn(|app| {
        app.map_get("/items/{id}", |id: u64| {
            Err::<Json<u64>, _>(ApiError::NotFound(id))
        });
    })
    .await;

    assert_eq!(
        get(&server, "/items/7").await,
        (
            404,
            "application/json".into(),
            r#"{"code":"not_found"}"#.into()
        )
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_lets_the_error_handler_keep_or_replace_the_response_an_error_carries() {
    let server = TestServer::spawn(|app| {
        app.map_err(|e: Error| async move {
            if e.instance().is_some_and(|uri| uri.ends_with("/replace")) {
                return status!(e.status().as_u16(), "replaced: {e}");
            }
            assert!(e.has_response());
            Err(e)
        });
        app.map_get("/keep", || Err::<&'static str, _>(ApiError::NotFound(1)));
        app.map_get("/replace", || Err::<&'static str, _>(ApiError::NotFound(2)));
    })
    .await;

    assert_eq!(
        get(&server, "/keep").await,
        (
            404,
            "application/json".into(),
            r#"{"code":"not_found"}"#.into()
        )
    );
    assert_eq!(
        get(&server, "/replace").await,
        (
            404,
            "text/plain; charset=utf-8".into(),
            "replaced: item 2 not found".into()
        )
    );

    server.shutdown().await;
}

// A `Problem` is as large as it is, and an `Err` of one is what these tests are about
#[cfg(feature = "problem-details")]
#[allow(clippy::result_large_err)]
mod problem_details {
    use super::{ApiError, get};
    use serde::Serialize;
    use volga::{
        error::{Error, Problem, ProblemDetails},
        http::StatusCode,
        test::TestServer,
    };

    #[derive(Default, Serialize)]
    struct Invalid {
        field: &'static str,
    }

    #[tokio::test]
    async fn it_describes_a_status_code_err_as_a_problem() {
        let server = TestServer::spawn(|app| {
            app.use_problem_details();
            app.map_get("/missing", || Err::<&'static str, _>(StatusCode::NOT_FOUND));
        })
        .await;

        let (status, content_type, body) = get(&server, "/missing").await;
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(status, 404);
        assert_eq!(content_type, "application/problem+json");
        assert_eq!(body["title"], "Not Found");
        assert_eq!(body["detail"], "Not Found");
        assert!(body["instance"].as_str().unwrap().ends_with("/missing"));

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_answers_a_problem_err_with_the_problem_itself() {
        let server = TestServer::spawn(|app| {
            app.map_get("/problem", || {
                Err::<&'static str, _>(
                    Problem::new(422)
                        .with_detail("field is invalid")
                        .with_extensions(Invalid { field: "name" }),
                )
            });
        })
        .await;

        let (status, content_type, body) = get(&server, "/problem").await;
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(status, 422);
        assert_eq!(content_type, "application/problem+json");
        assert_eq!(body["detail"], "field is invalid");
        assert_eq!(body["field"], "name");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_keeps_the_response_an_error_carries_under_problem_details() {
        let server = TestServer::spawn(|app| {
            app.use_problem_details();
            app.map_get("/problem", || {
                Err::<&'static str, _>(Problem::new(422).with_extensions(Invalid { field: "name" }))
            });
            app.map_get("/envelope", || {
                Err::<&'static str, _>(ApiError::NotFound(3))
            });
        })
        .await;

        let (status, _, body) = get(&server, "/problem").await;
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(status, 422);
        assert_eq!(body["field"], "name");

        assert_eq!(
            get(&server, "/envelope").await,
            (
                404,
                "application/json".into(),
                r#"{"code":"not_found"}"#.into()
            )
        );

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_hands_a_problem_err_to_the_error_handler() {
        let server = TestServer::spawn(|app| {
            app.map_err(|e: Error| async move {
                volga::status!(e.status().as_u16(), "{} at {:?}", e, e.instance())
            });
            app.map_get("/problem", || {
                Err::<&'static str, _>(
                    ProblemDetails::new(409)
                        .with_detail("taken")
                        .with_instance("/users/1"),
                )
            });
        })
        .await;

        assert_eq!(
            get(&server, "/problem").await,
            (
                409,
                "text/plain; charset=utf-8".into(),
                r#"taken at Some("/users/1")"#.into()
            )
        );

        server.shutdown().await;
    }
}

#[cfg(feature = "openapi")]
mod openapi {
    use serde_json::Value;
    use volga::{
        Json,
        error::{Error, IntoError},
        http::StatusCode,
        openapi::OpenApiRouteConfig,
        test::TestServer,
    };

    struct NotFound;

    impl IntoError for NotFound {
        fn into_error(self) -> Error {
            Error::from_parts(StatusCode::NOT_FOUND, None, "not found")
        }

        fn describe_openapi(config: OpenApiRouteConfig) -> OpenApiRouteConfig {
            config.produces_text(404)
        }
    }

    /// Converted with `?` as well, through the `From` that `IntoError` gives
    struct Taken;

    impl IntoError for Taken {
        fn into_error(self) -> Error {
            Error::from_parts(StatusCode::CONFLICT, None, "taken")
        }

        fn describe_openapi(config: OpenApiRouteConfig) -> OpenApiRouteConfig {
            config.produces_text(409)
        }
    }

    #[tokio::test]
    async fn it_describes_the_responses_an_error_declares() {
        let server = TestServer::builder()
            .configure(|app| app.with_open_api(|config| config))
            .setup(|app| {
                app.map_get("/declared", || Ok::<_, NotFound>(Json(1u8)));
                app.map_get("/taken", || Ok::<_, Taken>(Json(1u8)));
                app.map_get("/undeclared", || Ok::<_, String>(Json(1u8)));
                app.use_open_api();
            })
            .build()
            .await;

        let spec: Value = server
            .client()
            .get(server.url("/openapi.json"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let declared = spec["paths"]["/declared"]["get"]["responses"]
            .as_object()
            .unwrap();
        let undeclared = spec["paths"]["/undeclared"]["get"]["responses"]
            .as_object()
            .unwrap();

        let taken = spec["paths"]["/taken"]["get"]["responses"]
            .as_object()
            .unwrap();

        assert!(declared.contains_key("200"));
        assert!(declared["404"]["content"]["text/plain; charset=utf-8"].is_object());
        assert!(taken["409"]["content"]["text/plain; charset=utf-8"].is_object());
        assert_eq!(undeclared.keys().collect::<Vec<_>>(), ["200"]);

        server.shutdown().await;
    }
}

#[cfg(feature = "ws")]
mod ws {
    use volga::{
        error::{Error, IntoError},
        http::StatusCode,
        test::TestServer,
        ws::Message,
    };

    /// An incoming message type with a conversion error of its own
    struct Shout(String);

    struct NotText;

    impl IntoError for NotText {
        fn into_error(self) -> Error {
            Error::from_parts(StatusCode::BAD_REQUEST, None, "not text")
        }
    }

    impl TryFrom<Message> for Shout {
        type Error = NotText;

        fn try_from(msg: Message) -> Result<Self, NotText> {
            String::try_from(msg)
                .map(|text| Shout(text.to_uppercase()))
                .map_err(|_| NotText)
        }
    }

    /// A reply type with a conversion error of its own
    struct Reply(String);

    struct Unsendable;

    impl IntoError for Unsendable {
        fn into_error(self) -> Error {
            Error::server_error("unsendable")
        }
    }

    impl TryFrom<Reply> for Message {
        type Error = Unsendable;

        fn try_from(reply: Reply) -> Result<Self, Unsendable> {
            Message::try_from(reply.0).map_err(|_| Unsendable)
        }
    }

    #[tokio::test]
    async fn it_converts_messages_with_errors_of_their_own() {
        let server = TestServer::spawn(|app| {
            app.map_msg("/ws", |msg: Shout| async move { Reply(msg.0) });
            app.map_msg("/sync", |msg: Shout| Reply(msg.0));
        })
        .await;

        for path in ["/ws", "/sync"] {
            let mut ws = server.ws(path).await;
            ws.send_text("hi").await;
            assert_eq!(ws.recv_text().await, "HI", "{path}");
        }

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_replies_with_a_message() {
        let server = TestServer::spawn(|app| {
            app.map_msg("/ws", |msg: String| Message::try_from(msg).unwrap());
        })
        .await;

        let mut ws = server.ws("/ws").await;
        ws.send_text("as is").await;
        assert_eq!(ws.recv_text().await, "as is");

        server.shutdown().await;
    }
}

#[cfg(feature = "middleware")]
mod tap_req {
    use volga::{
        HttpRequestMut,
        error::{Error, IntoError},
        http::StatusCode,
        test::TestServer,
    };

    struct Forbidden;

    impl IntoError for Forbidden {
        fn into_error(self) -> Error {
            Error::from_parts(StatusCode::FORBIDDEN, None, "forbidden")
        }
    }

    fn check(req: &HttpRequestMut) -> Result<(), Forbidden> {
        if req.uri().path().ends_with("/deny") {
            return Err(Forbidden);
        }
        Ok(())
    }

    #[tokio::test]
    async fn it_converts_an_error_of_its_own_in_tap_req() {
        let server = TestServer::spawn(|app| {
            // `tap_req` takes `Result<HttpRequestMut, Error>` alone, so a bare `Ok(..)` after a
            // `?` needs no annotation, and `?` converts through the `From` `IntoError` gives
            app.map_get("/question-mark/{x}", |x: String| x).tap_req(
                |req: HttpRequestMut| async move {
                    check(&req)?;
                    Ok(req)
                },
            );
            app.map_get("/into/{x}", |x: String| x)
                .tap_req(|req: HttpRequestMut| {
                    if req.uri().path().ends_with("/deny") {
                        return Err(Forbidden.into());
                    }
                    Ok(req)
                });
        })
        .await;

        for (path, status) in [
            ("/question-mark/ok", 200),
            ("/question-mark/deny", 403),
            ("/into/ok", 200),
            ("/into/deny", 403),
        ] {
            let res = server.client().get(server.url(path)).send().await.unwrap();
            assert_eq!(res.status().as_u16(), status, "{path}");
        }

        server.shutdown().await;
    }
}
