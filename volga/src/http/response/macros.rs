//! Macros for various HTTP responses

/// Produces HTTP 404 NOT FOUND response
///
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::not_found;
///
/// not_found!();
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::not_found;
///
/// not_found!("User not found!");
/// ```
#[macro_export]
macro_rules! not_found {
    () => {
        $crate::status!(404)
    };
    ([ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404; [ $( $header ),* ])
    };

    (text: $body:expr) => {
        $crate::status!(404, text: $body)
    };
    (text: $body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, text: $body; [ $( $header ),* ])
    };

    (fmt: $body:literal) => {
        $crate::status!(404, fmt: $body)
    };
    (fmt: $body:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, fmt: $body; [ $( $header ),* ])
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::status!(404, fmt: $body, $( $arg ),+)
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, fmt: $body, $( $arg ),+ ; [ $( $header ),* ])
    };

    (json: $text:expr) => {
        $crate::status!(404, json: $text)
    };
    (json: $text:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, json: $text; [ $( $header ),* ])
    };

    ({ $($json:tt)* }) => {
        $crate::status!(404, { $($json)* })
    };
    ({ $($json:tt)* }; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, { $($json)* }; [ $( $header ),* ])
    };

    ($text:literal) => {
        $crate::status!(404, $text)
    };
    ($text:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, $text; [ $( $header ),* ])
    };
    ($text:literal, $( $arg:expr ),+ $(,)?) => {
        $crate::status!(404, $text, $( $arg ),+)
    };
    ($text:literal, $( $arg:expr ),+ $(,)?; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, $text, $( $arg ),+; [ $( $header ),* ])
    };

    ($body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(404, $body; [ $( $header ),* ])
    };
    ($body:expr) => {
        $crate::status!(404, $body)
    };
}

/// Produces HTTP 400 BAD REQUEST response
///
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::bad_request;
///
/// bad_request!();
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::bad_request;
///
/// bad_request!("User not found!");
/// ```
#[macro_export]
macro_rules! bad_request {
    () => {
        $crate::status!(400)
    };
    ([ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400; [ $( $header ),* ])
    };

    (text: $body:expr) => {
        $crate::status!(400, text: $body)
    };
    (text: $body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, text: $body; [ $( $header ),* ])
    };

    (fmt: $body:literal) => {
        $crate::status!(400, fmt: $body)
    };
    (fmt: $body:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, fmt: $body; [ $( $header ),* ])
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::status!(400, fmt: $body, $( $arg ),+)
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, fmt: $body, $( $arg ),+ ; [ $( $header ),* ])
    };

    (json: $text:expr) => {
        $crate::status!(400, json: $text)
    };
    (json: $text:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, json: $text; [ $( $header ),* ])
    };

    ({ $($json:tt)* }) => {
        $crate::status!(400, { $($json)* })
    };
    ({ $($json:tt)* }; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, { $($json)* }; [ $( $header ),* ])
    };

    ($text:literal) => {
        $crate::status!(400, $text)
    };
    ($text:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, $text; [ $( $header ),* ])
    };
    ($text:literal, $( $arg:expr ),+ $(,)?) => {
        $crate::status!(400, $text, $( $arg ),+)
    };
    ($text:literal, $( $arg:expr ),+ $(,)?; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, $text, $( $arg ),+; [ $( $header ),* ])
    };

    ($body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(400, $body; [ $( $header ),* ])
    };
    ($body:expr) => {
        $crate::status!(400, $body)
    };
}

/// Produces HTTP 201 CREATED response
///
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::created;
///
/// created!();
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::created;
///
/// created!("User created!");
/// ```
#[macro_export]
macro_rules! created {
    () => {
        $crate::status!(201)
    };
    ([ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201; [ $( $header ),* ])
    };

    (text: $body:expr) => {
        $crate::status!(201, text: $body)
    };
    (text: $body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, text: $body; [ $( $header ),* ])
    };

    (fmt: $body:literal) => {
        $crate::status!(201, fmt: $body)
    };
    (fmt: $body:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, fmt: $body; [ $( $header ),* ])
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::status!(201, fmt: $body, $( $arg ),+)
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, fmt: $body, $( $arg ),+ ; [ $( $header ),* ])
    };

    (json: $text:expr) => {
        $crate::status!(201, json: $text)
    };
    (json: $text:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, json: $text; [ $( $header ),* ])
    };

    ({ $($json:tt)* }) => {
        $crate::status!(201, { $($json)* })
    };
    ({ $($json:tt)* }; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, { $($json)* }; [ $( $header ),* ])
    };

    ($text:literal) => {
        $crate::status!(201, $text)
    };
    ($text:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, $text; [ $( $header ),* ])
    };
    ($text:literal, $( $arg:expr ),+ $(,)?) => {
        $crate::status!(201, $text, $( $arg ),+)
    };
    ($text:literal, $( $arg:expr ),+ $(,)?; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, $text, $( $arg ),+; [ $( $header ),* ])
    };

    ($body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(201, $body; [ $( $header ),* ])
    };
    ($body:expr) => {
        $crate::status!(201, $body)
    };
}

/// Produces HTTP 202 ACCEPTED response
///
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::accepted;
///
/// accepted!();
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::accepted;
///
/// accepted!("Task accepted!");
/// ```
#[macro_export]
macro_rules! accepted {
    () => {
        $crate::status!(202)
    };
    ([ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202; [ $( $header ),* ])
    };

    (text: $body:expr) => {
        $crate::status!(202, text: $body)
    };
    (text: $body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, text: $body; [ $( $header ),* ])
    };

    (fmt: $body:literal) => {
        $crate::status!(202, fmt: $body)
    };
    (fmt: $body:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, fmt: $body; [ $( $header ),* ])
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::status!(202, fmt: $body, $( $arg ),+)
    };
    (fmt: $body:literal, $( $arg:expr ),+ $(,)? ; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, fmt: $body, $( $arg ),+ ; [ $( $header ),* ])
    };

    (json: $text:expr) => {
        $crate::status!(202, json: $text)
    };
    (json: $text:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, json: $text; [ $( $header ),* ])
    };

    ({ $($json:tt)* }) => {
        $crate::status!(202, { $($json)* })
    };
    ({ $($json:tt)* }; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, { $($json)* }; [ $( $header ),* ])
    };

    ($text:literal) => {
        $crate::status!(202, $text)
    };
    ($text:literal; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, $text; [ $( $header ),* ])
    };
    ($text:literal, $( $arg:expr ),+ $(,)?) => {
        $crate::status!(202, $text, $( $arg ),+)
    };
    ($text:literal, $( $arg:expr ),+ $(,)?; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, $text, $( $arg ),+; [ $( $header ),* ])
    };

    ($body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::status!(202, $body; [ $( $header ),* ])
    };
    ($body:expr) => {
        $crate::status!(202, $body)
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use serde::Serialize;
    use bytes::Bytes;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }

    async fn body_to_bytes(resp: &mut crate::http::HttpResponse) -> Bytes {
        resp.body_mut().collect().await.unwrap().to_bytes()
    }

    fn assert_header(resp: &crate::http::HttpResponse, k: &str, v: &str) {
        assert_eq!(resp.headers().get(k).unwrap(), v);
    }
    
    #[tokio::test]
    async fn it_creates_400_response() {
        let response = bad_request!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_text_response() {
        let text = "test";
        let response = bad_request!(text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_interpolated_text_response() {
        let text = "test";
        let response = bad_request!("Error: {text}");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Error: test");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_formatted_text_response() {
        let text = "test";
        let response = bad_request!("Error: {}", text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Error: test");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = bad_request!(payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_400_response_with_json_body() {
        let response = bad_request!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_400_response_with_json_body_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = bad_request!(payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_text_prefixed_response() {
        let response = bad_request!(text: "test");
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "test");
        assert_eq!(response.status(), 400);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_400_with_text_prefixed_response_and_headers() {
        let response = bad_request!(text: "test"; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "test");
        assert_eq!(response.status(), 400);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_400_with_fmt_prefixed_capture() {
        let text = "test";
        let response = bad_request!(fmt: "Error: {text}");
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "Error: test");
        assert_eq!(response.status(), 400);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_400_with_fmt_prefixed_args() {
        let response = bad_request!(fmt: "Error: {} {}", "a", 1);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "Error: a 1");
        assert_eq!(response.status(), 400);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_400_with_fmt_prefixed_args_and_headers() {
        let response = bad_request!(fmt: "Error: {}", "x"; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "Error: x");
        assert_eq!(response.status(), 400);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_400_with_json_body_and_headers_via_braces() {
        let response = bad_request!({ "name": "test" }; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 400);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }

    
    #[tokio::test]
    async fn it_creates_404_response() {
        let response = not_found!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_text() {
        let response = not_found!("User not found");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User not found");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_interpolated_text() {
        let user = "User";
        let response = not_found!("{user} not found");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User not found");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_formatted_text() {
        let user = "User";
        let response = not_found!("{} not found", user);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User not found");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = not_found!(payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_404_response_with_json_body() {
        let response = not_found!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_headers() {
        let response = not_found!([
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 404);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_404_response_with_json_body_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = not_found!(payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_with_text_prefixed_response() {
        let response = not_found!(text: "User not found");
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "User not found");
        assert_eq!(response.status(), 404);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_404_with_fmt_prefixed_capture_and_headers() {
        let user = "User";
        let response = not_found!(fmt: "{user} not found"; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "User not found");
        assert_eq!(response.status(), 404);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_404_with_fmt_prefixed_args() {
        let response = not_found!(fmt: "{} {}", "User", "not found");
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "User not found");
        assert_eq!(response.status(), 404);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_404_with_json_prefixed_response_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = not_found!(json: payload; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 404);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_201_response() {
        let response = created!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_text() {
        let response = created!("User created");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User created");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_interpolated_text() {
        let user = "User";
        let response = created!("{user} created");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User created");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_formatted_text() {
        let user = "User";
        let response = created!("{} created", user);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "User created");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = created!(payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_201_response_with_json_body() {
        let response = created!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_headers() {
        let response = created!([
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 201);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_201_response_with_json_body_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = created!(payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_with_text_prefixed_response() {
        let response = created!(text: "User created");
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "User created");
        assert_eq!(response.status(), 201);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_201_with_fmt_prefixed_args_and_headers() {
        let response = created!(fmt: "{} created", "User"; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "User created");
        assert_eq!(response.status(), 201);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_201_with_json_prefixed_response_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = created!(json: payload; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 201);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_201_with_json_body_and_headers_via_braces() {
        let response = created!({ "name": "test" }; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 201);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }
    
    #[tokio::test]
    async fn it_creates_202_response() {
        let response = accepted!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_text() {
        let response = accepted!("Task accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_text_text() {
        let response = accepted!(text: "Task accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_fmt_text() {
        let response = accepted!(fmt: "Task accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_interpolated_text() {
        let task = "Task";
        let response = accepted!("{task} accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_formatted_text() {
        let task = "Task";
        let response = accepted!("{} accepted", task);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_fmt_interpolated_text() {
        let task = "Task";
        let response = accepted!(fmt: "{task} accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_fmt_formatted_text() {
        let task = "Task";
        let response = accepted!(fmt: "{} accepted", task);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "Task accepted");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = accepted!(payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_202_response_with_json_body() {
        let response = accepted!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_headers() {
        let response = accepted!([
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 202);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_202_response_with_json_body_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = accepted!(payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_with_fmt_prefixed_capture_and_headers() {
        let task = "Task";
        let response = accepted!(fmt: "{task} accepted"; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "Task accepted");
        assert_eq!(response.status(), 202);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn it_creates_202_with_json_prefixed_response() {
        let payload = TestPayload { name: "test".into() };
        let response = accepted!(json: payload);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 202);
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_202_with_json_prefixed_response_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = accepted!(json: payload; [("x-req-id", "1")]);
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = body_to_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(&body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 202);
        assert_header(&response, "x-req-id", "1");
        assert_eq!(response.headers().get("content-type").unwrap(), "application/json");
    }
}