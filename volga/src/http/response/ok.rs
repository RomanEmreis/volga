//! Macros for `OK 200` HTTP responses.

/// Creates a `200 OK` response.
///
/// The macro provides three “modes”:
///
/// - **Empty response**: `ok!()`
/// - **Plain text (UTF-8)**:
///   - Sugar for string literals: `ok!("...")`, `ok!("...", args...)`
///   - Explicit: `ok!(text: ...)` (works for any value via `ToString`)
/// - **JSON**:
///   - Typed: `ok!(value)` (serializes `value` as JSON)
///   - Untyped object sugar: `ok!({ ... })`
///   - Explicit: `ok!(json: value)`
///
/// # Content-Type rules
///
/// - `ok!()` produces an empty body and does **not** set `Content-Type`.
/// - `ok!("...")` / `ok!("...", args...)` sets:
///   - `Content-Type: text/plain; charset=utf-8`
/// - `ok!(text: ...)` sets:
///   - `Content-Type: text/plain; charset=utf-8`
/// - JSON variants set:
///   - `Content-Type: application/json`
///
/// # Important notes
///
/// - The `ok!("...")` form is **intended for string literals**.
///   In Rust macros, numeric/bool literals can also match this pattern; in such cases
///   prefer the explicit forms:
///   - `ok!(text: true)` / `ok!(text: 150)`
///   - `ok!(json: true)` / `ok!(json: 150)`
///
/// # Examples
///
/// ## Plain text
/// ```no_run
/// use volga::ok;
///
/// ok!("healthy");
/// ok!("Hello, {}!", "world");
/// ```
///
/// ## Plain text with custom headers
/// ```no_run
/// use volga::ok;
///
/// ok!("ok"; [
///     ("x-api-key", "some api key"),
///     ("x-req-id", "some req id"),
/// ]);
///
/// ok!(text: true; [("x-flag", "1")]);
/// ok!(text: 150);
/// ```
///
/// ## Without body
/// ```no_run
/// use volga::ok;
///
/// ok!();
/// ok!([("x-req-id", "some req id")]);
/// ```
///
/// ## JSON (typed)
/// ```no_run
/// use volga::ok;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Health {
///     status: String,
/// }
///
/// let health = Health { status: "healthy".into() };
/// ok!(health);
/// ```
///
/// ## JSON (untyped object sugar)
/// ```no_run
/// use volga::ok;
///
/// ok!({ "health": "healthy" });
/// ```
///
/// ## JSON with custom headers
/// ```no_run
/// use volga::ok;
///
/// ok!({ "health": "healthy" }; [
///     ("x-api-key", "some api key"),
/// ]);
///
/// ok!(json: "ok"); // JSON string: "ok"
/// ok!(json: true);
/// ```
#[macro_export]
macro_rules! ok {
    // =========================
    // 1) Empty / headers-only
    // =========================

    // ok!()
    () => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::empty()
        )
    };

    // ok!([("k","v"), ...])   -- headers on empty body
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::empty();
            [ $( ($key, $value) ),* ]
        )
    };

    // ok!([header, ...])   -- headers on empty body
    ([ $( $header:expr ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::empty();
            [ $( $header ),* ]
        )
    };

    // =========================
    // 2) Explicit TEXT (ToString)
    // =========================

    // ok!(text: expr)
    (text: $body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $body.into();
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // ok!(text: expr; [headers])
    (text: $body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $body.into();
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // ok!(text: expr; [headers])
    (text: $body:expr ; [ $( $header:expr ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $body.into();
            [
                $crate::headers::ContentType::from_static("text/plain; charset=utf-8"),
                $( $header ),*
            ]
        )
    };

    // =========================
    // 3) Explicit FMT (format!)
    //    Use expr args to avoid capturing headers.
    // =========================

    // ok!(fmt: "hello {name}")
    (fmt: $fmt:literal) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // ok!(fmt: "hello {name}"; [headers])
    (fmt: $fmt:literal ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt));
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // ok!(fmt: "hello {} {}", name, 123)
    (fmt: $fmt:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // ok!(fmt: "hello {}", name; [headers])
    (fmt: $fmt:literal, $( $arg:expr ),+ $(,)? ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // =========================
    // 4) Explicit JSON
    // =========================

    // ok!(json: expr)
    (json: $body:expr) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // ok!(json: expr; [headers])
    (json: $body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [
                    ($crate::headers::CONTENT_TYPE, "application/json"),
                    $( ($key, $value) ),*
                ]
            ),
            Err(err) => Err(err),
        }
    }};

    // =========================
    // 5) JSON object sugar
    // =========================

    // ok!({ ... })
    ({ $($json:tt)* }) => {{
        match $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // ok! { k: v, ... }
    { $($name:tt : $value:tt),* $(,)? } => {{
        match $crate::HttpBody::json($crate::json::json_internal!({ $($name: $value),* })) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // ok!({ ... }; [headers])
    ({ $($json:tt)* } ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [
                    ($crate::headers::CONTENT_TYPE, "application/json"),
                    $( ($key, $value) ),*
                ]
            ),
            Err(err) => Err(err),
        }
    }};

    // =========================
    // 6) Text sugar for string literals (no prefix)
    //    NOTE: this still matches non-string literals too (known limitation).
    // =========================

    // ok!("ok")
    ($fmt:literal) => {{
        const __S: &str = $fmt;

        if $crate::utils::str::memchr_contains(b'{', __S.as_bytes()) {
            $crate::response!(
                $crate::http::StatusCode::OK,
                $crate::HttpBody::full(format!($fmt));
                [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
            )
        } else {
            $crate::response!(
                $crate::http::StatusCode::OK,
                $crate::HttpBody::text(__S);
                [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
            )
        }
    }};

    // ok!("ok"; [headers])
    ($fmt:literal ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        const __S: &str = $fmt;

        if $crate::utils::str::memchr_contains(b'{', __S.as_bytes()) {
            $crate::response!(
                $crate::http::StatusCode::OK,
                $crate::HttpBody::full(format!($fmt));
                [
                    ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                    $( ($key, $value) ),*
                ]
            )
        } else {
            $crate::response!(
                $crate::http::StatusCode::OK,
                $crate::HttpBody::text(__S);
                [
                    ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                    $( ($key, $value) ),*
                ]
            )
        }
    }};

    // ok!("Hello {}", name)
    ($fmt:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // ok!("Hello {}", name; [headers])
    ($fmt:literal, $( $arg:expr ),+ $(,)? ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // =========================
    // 7) Fallback: JSON for expr
    // =========================

    // ok!(expr)
    ($body:expr) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // ok!(expr; [headers])
    ($body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::OK,
                body;
                [
                    ($crate::headers::CONTENT_TYPE, "application/json"),
                    $( ($key, $value) ),*
                ]
            ),
            Err(err) => Err(err),
        }
    }};
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use http_body_util::BodyExt;
    use serde::Serialize;

    use crate::headers;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }

    headers! {
        (ApiKey, "x-api-key"),
        (RequestId, "x-req-id")
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
    async fn it_creates_anonymous_type_json_ok_variant_1_response() {
        let response = ok!({ "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_json_ok_variant_2_response() {
        let response = ok! { 
            "name_1": 1,
            "name_2": "test 2"
        };

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name_1\":1,\"name_2\":\"test 2\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
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
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_literal_text_ok_response() {
        let response = ok!("test");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_expr_ok_response() {
        let response = ok!(5 + 5);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "10");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_number_ok_response() {
        let number = 100;
        let response = ok!(number);
        // this is known issue will be fixed in future releases.
        // let response = ok!(100);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "100");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_boolean_ok_response() {
        let response = ok!(text: true);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
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
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
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
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_formatted_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {}", text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "This is text: test");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_interpolated_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {text}");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "This is text: test");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_empty_ok_response() {
        let response = ok!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert!(response.headers().get("Content-Type").is_none());
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {
        let response = ok!("ok"; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "ok");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_text_response_with_empty_custom_headers() {
        #[allow(unused_mut)]
        let response = ok!("ok"; []);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "ok");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.headers().len(), 2);
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn in_creates_json_response_with_custom_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_anonymous_json_response_with_custom_headers() {
        let response = ok!({ "name": "test" }; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_empty_ok_response_with_headers() {
        let response = ok!([
            ApiKey::from_static("some api key"),
            RequestId::from_static("some req id")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
        assert!(response.headers().get("Content-Type").is_none());
    }

    #[tokio::test]
    async fn it_creates_empty_ok_response_with_raw_headers() {
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
        assert!(response.headers().get("Content-Type").is_none());
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_string_ok_response() {
        let response = ok!(text: "test");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_number_ok_response() {
        let response = ok!(text: 150);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "150");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_char_ok_response() {
        let response = ok!(text: 'a');

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "a");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_response_with_custom_headers() {
        let response = ok!(text: "ok"; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "ok");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_response_with_headers() {
        let response = ok!(text: "ok"; [
            ApiKey::from_static("some api key"),
            RequestId::from_static("some req id")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "ok");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_text_prefixed_formatted_response_with_custom_headers() {
        let name = "volga";
        let response = ok!(fmt: "hello {}", name; [
            ("x-req-id", "123"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "hello volga");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-req-id").unwrap(), "123");
    }

    #[tokio::test]
    async fn it_creates_explicit_json_ok_response() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(json: payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_explicit_json_string_ok_response() {
        let response = ok!(json: "ok");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        // JSON string, not plain text
        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_explicit_json_response_with_custom_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(json: payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}