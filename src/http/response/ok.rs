/// Produces an `OK 200` response with plain text or JSON body
/// 
/// # Examples
/// ## plain/text
/// ```no_run
/// use volga::ok;
///
/// ok!("healthy");
/// ```
/// ## plain/text without body
/// ```no_run
/// use volga::ok;
///
/// ok!();
/// ```
/// ## JSON
///```no_run
/// use volga::ok;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Health {
///    status: String
/// }
///
/// let health = Health { status: "healthy".into() };
/// ok!(health);
/// ```
/// ## Untyped JSON with custom headers
///```no_run
/// use volga::ok;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Health {
///    status: String
/// }
///
/// ok!({ "health": "healthy" }, [
///    ("x-api-key", "some api key")
/// ]);
/// ```
#[macro_export]
macro_rules! ok {
    // handles ok!()
    () => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::empty(),
            [
                ($crate::headers::CONTENT_TYPE, "text/plain")
            ]
        )
    };
    
    // handles ok!([("key", "val")])
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::empty(),
            [ $( ($key, $value) ),* ]
        )
    };
    
    // handles ok!({ json })
    ({ $($json:tt)* }) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    // handles ok!({ json }, [("key", "val")])
    ({ $($json:tt)* }, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
                $( ($key, $value) ),*
            ]
        )
    };
    
    // handles ok!(json)
    ($var:ident) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json($var),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    // handles ok!(json, [("key", "val")])
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json($body),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
                $( ($key, $value) ),*
            ]
        )
    };
    
    // handles ok!("Hello {name}")
    ($fmt:tt) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json(format!($fmt)),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    // handles ok!(thing.to_string()) or ok!(5 + 5)
    ($body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json($body),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    // handles ok!("Hello {}", name)
    ($($fmt:tt)*) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::json(format!($($fmt)*)),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }

    #[tokio::test]
    async fn it_creates_json_ok_response() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(payload);

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_json_from_inline_struct_ok_response() {
        let response = ok!(TestPayload { name: "test".into() });

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_json_ok_response() {
        let response = ok!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_text_ok_response() {
        let text = "test";
        let response = ok!(text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_literal_text_ok_response() {
        let response = ok!("test");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_expr_ok_response() {
        let response = ok!(5 + 5);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "10");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_number_ok_response() {
        let number = 100;
        let response = ok!(number);
        // this is known issue will be fixed in future releases.
        //let response = ok!(100);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "100");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_boolean_ok_response() {
        let response = ok!(true);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_char_ok_response() {
        let ch = 'a';
        let response = ok!(ch);
        // this is known issue will be fixed in future releases.
        //let response = ok!('a');

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"a\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_array_ok_response() {
        let vec = vec![1,2,3];
        let response = ok!(vec);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "[1,2,3]");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_formatted_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {}", text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"This is text: test\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_interpolated_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {text}");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"This is text: test\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_empty_ok_response() {
        let response = ok!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {
        let response = ok!("ok", [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_text_response_with_empty_custom_headers() {
        #[allow(unused_mut)]
        let response = ok!("ok", []);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(response.headers().len(), 2);
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn in_creates_json_response_with_custom_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(payload, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_anonymous_json_response_with_custom_headers() {
        let response = ok!({ "name": "test" }, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_empty_ok_response_with_headers() {
        let response = ok!([
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}