use hyper::StatusCode;
use volga::{App, Results};
use volga::error::Error;
use volga::headers::Headers;

#[tokio::test]
async fn it_adds_middleware_request() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7884");

        app.use_middleware(|context, next| async move {
            next(context).await
        });
        app.use_middleware(|_, _| async move {
            Results::text("Pass!")
        });

        app.map_get("/test", || async {
            Results::text("Unreachable!")
        });

       app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7884/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_ok_middleware() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7942");

        app.map_ok(|mut resp| async move {
            resp.headers_mut().insert("X-Test", "Test".parse().unwrap());
            resp
        });

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
        client.get("http://127.0.0.1:7942/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_req_middleware() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7943");

        app.map_request(|mut req| async move {
            req.headers_mut().insert("X-Test", "Pass!".parse().unwrap());
            req
        });

        app.map_get("/test", |headers: Headers| async move {
            let val = headers.get("X-Test").unwrap().to_str().unwrap();
            Results::text(val)
        });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7943/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_ok_middleware_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7944");

        app.map_get("/test", || async {
                Results::text("Pass!")
            })
            .map_ok(|mut resp| async move {
                resp.headers_mut().insert("X-Test", "Test".parse().unwrap());
                resp
            });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7944/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_req_middleware_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7945");

        app.map_get("/test", |headers: Headers| async move {
                let val = headers.get("X-Test").unwrap().to_str().unwrap();
                Results::text(val)
            })
            .map_request(|mut req| async move {
                req.headers_mut().insert("X-Test", "Pass!".parse().unwrap());
                req
            });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7945/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_ok_middleware_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7946");

        app.map_group("/tests")
            .map_ok(|mut resp| async move {
                resp.headers_mut().insert("X-Test", "Test".parse().unwrap());
                resp
            })
            .map_get("/test", || async {
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
        client.get("http://127.0.0.1:7946/tests/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_req_middleware_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7947");

        app.map_group("/tests")
            .map_request(|mut req| async move {
                req.headers_mut().insert("X-Test", "Pass!".parse().unwrap());
                req
            })
            .map_get("/test", |headers: Headers| async move {
                let val = headers.get("X-Test").unwrap().to_str().unwrap();
                Results::text(val)
            });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7947/tests/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_map_err_middleware_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7948");

        app.map_get("/test", || async {
                Err::<String, Error>(Error::server_error("Some Error"))
            })
            .map_err(|err| async move {
                let mut err_str = err.to_string();
                err_str.push_str(" occurred!");
                Error::server_error(err_str)
            });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7948/test").send().await
    }).await.unwrap().unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "\"Some Error occurred!\"");
}

#[tokio::test]
async fn it_adds_map_err_middleware_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7949");

        app.map_group("/tests")
            .map_err(|err| async move {
                let mut err_str = err.to_string();
                err_str.push_str(" occurred!");
                Error::server_error(err_str)
            })
            .map_get("/test", || async {
                Err::<String, Error>(Error::server_error("Some Error"))
            });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7949/tests/test").send().await
    }).await.unwrap().unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "\"Some Error occurred!\"");
}

#[tokio::test]
async fn it_adds_invalid_filter_middleware_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7950");

        app
            .map_get("/test", || async {})
            .filter(|| async move { false });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7950/test").send().await
    }).await.unwrap().unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.text().await.unwrap(), "\"Validation: One or more request parameters are incorrect\"");
}

#[tokio::test]
async fn it_adds_valid_filter_middleware_for_route() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7951");

        app
            .map_get("/test", || async { "Pass!" })
            .filter(|| async move { true });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7951/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_adds_invalid_filter_middleware_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7952");

        app.map_group("/tests")
            .filter(|| async move { false })
            .map_get("/test", || async {});

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7952/tests/test").send().await
    }).await.unwrap().unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.text().await.unwrap(), "\"Validation: One or more request parameters are incorrect\"");
}

#[tokio::test]
async fn it_adds_valid_filter_middleware_for_group() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7953");

        app.map_group("/tests")
            .filter(|| async move { true })
            .map_get("/test", || async { "Pass!" });

        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7953/tests/test").send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
}