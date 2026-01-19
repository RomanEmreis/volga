//! Macros for responses with various HTTP statuses.

/// Produces a response with the specified HTTP status code.
///
/// The macro supports three “modes”:
///
/// - **Empty response**: `status!(404)`
/// - **Plain text (UTF-8)**:
///   - Sugar for string literals: `status!(401, "...")`, `status!(401, "...", args...)`
///   - Explicit: `status!(401, text: ...)` (works for any value via `ToString`)
///   - Explicit formatted: `status!(401, textf: "...", args...)`
/// - **JSON**:
///   - Typed: `status!(401, value)` (serializes `value` as JSON)
///   - Untyped object sugar: `status!(401, { ... })`
///   - Explicit: `status!(401, json: value)`
///
/// # Custom headers
///
/// Custom headers are appended using a **semicolon separator**:
///
/// - `status!(401; [("x-req-id", "123")])`
/// - `status!(401, text: "Unauthorized!"; [("x-req-id", "123")])`
/// - `status!(401, textf: "Unauthorized: {}", reason; [("x-req-id", "123")])`
/// - `status!(401, json: payload; [("x-req-id", "123")])`
///
/// This form avoids macro ambiguities where headers could be accidentally captured as
/// formatting arguments.
///
/// # Examples
///
/// ## Without body
/// ```no_run
/// use volga::status;
///
/// status!(404);
/// status!(404; [("x-req-id", "123")]);
/// ```
///
/// ## text/plain body
/// ```no_run
/// use volga::status;
///
/// status!(401, "Unauthorized!");
/// status!(401, text: true);
/// status!(401, textf: "Unauthorized: {}", "token expired");
/// status!(401, "Unauthorized!"; [("x-req-id", "123")]);
/// status!(401, textf: "Unauthorized: {}", "token expired"; [("x-req-id", "123")]);
/// ```
///
/// ## JSON body
/// ```no_run
/// use volga::status;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct ErrorMessage {
///     error: String
/// }
///
/// let error = ErrorMessage { error: "some error message".into() };
/// status!(401, error);
/// status!(401, json: "ok"); // JSON string
/// status!(401, { "error": "some error message" });
/// ```
#[macro_export]
macro_rules! status {
    // =========================
    // 0) Helpers
    // =========================
    // NOTE: keep status parsing consistent everywhere
    // (macro_rules can't define locals, so we repeat the expression)

    // =========================
    // 1) Empty / headers-only
    // =========================

    // status!(404)
    ($status:expr) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::empty()
        )
    };

    // status!(404; [("k","v"), ...])
    ($status:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::empty();
            [ $( ($key, $value) ),* ]
        )
    };

    // =========================
    // 2) Explicit TEXT (ToString)
    // =========================

    // status!(401, text: expr)
    ($status:expr, text: $body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full($body.to_string());
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // status!(401, text: expr; [headers])
    ($status:expr, text: $body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full($body.to_string());
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // =========================
    // 3) Explicit TEXTF (format!)
    //    Use expr args to avoid capturing headers.
    // =========================

    // status!(401, textf: "literal")
    ($status:expr, textf: $fmt:literal) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // status!(401, textf: "literal"; [headers])
    ($status:expr, textf: $fmt:literal ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt));
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // status!(401, textf: "literal", args...)
    ($status:expr, textf: $fmt:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // status!(401, textf: "literal", args...; [headers])
    ($status:expr, textf: $fmt:literal, $( $arg:expr ),+ $(,)? ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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

    // status!(401, json: expr)
    ($status:expr, json: $body:expr) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // status!(401, json: expr; [headers])
    ($status:expr, json: $body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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

    // status!(401, { ... })
    ($status:expr, { $($json:tt)* }) => {{
        match $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // status!(401, { ... }; [headers])
    ($status:expr, { $($json:tt)* } ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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
    // 6) Plain text sugar (string-literal-first)
    //    Uses expr args to avoid capturing headers.
    // =========================

    // status!(401, "Unauthorized!")
    ($status:expr, $fmt:literal) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // status!(401, "Unauthorized!"; [headers])
    ($status:expr, $fmt:literal ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt));
            [
                ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };

    // status!(401, "Unauthorized: {}", reason)
    ($status:expr, $fmt:literal, $( $arg:expr ),+ $(,)? ) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::full(format!($fmt, $( $arg ),+));
            [ ($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8") ]
        )
    };

    // status!(401, "Unauthorized: {}", reason; [headers])
    ($status:expr, $fmt:literal, $( $arg:expr ),+ $(,)? ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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

    // status!(401, expr)
    ($status:expr, $body:expr) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
                body;
                [ ($crate::headers::CONTENT_TYPE, "application/json") ]
            ),
            Err(err) => Err(err),
        }
    }};

    // status!(401, expr; [headers])
    ($status:expr, $body:expr ; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        match $crate::HttpBody::json($body) {
            Ok(body) => $crate::response!(
                $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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
mod tests {
    use http_body_util::BodyExt;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }

    #[tokio::test]
    async fn it_creates_200_response() {
        let response = status!(200);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_200_with_text_response() {
        let text = "test";
        let response = status!(200, text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_200_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(200, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_200_response_with_json_body() {
        let response = status!(200, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_empty_401_response() {
        let response = status!(401);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_401_response_with_text_body() {
        let response = status!(401, "You are not authorized!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "You are not authorized!");
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_401_response_with_interpolated_text_body() {
        let name = "John";
        let response = status!(401, "{} is not authorized!", name);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "John is not authorized!");
        assert_eq!(response.status(), 401);
    }
    
    #[tokio::test]
    async fn it_creates_401_response_with_formatted_text_body() {
        let name = "John";
        let response = status!(401, "{name} is not authorized!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "John is not authorized!");
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_401_response_with_json_body() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(401, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_401_response_with_json_body() {
        let response = status!(401, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_empty_403_response() {
        let response = status!(403);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn it_creates_403_response_with_text_body() {
        let response = status!(403, "It's forbidden!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "It's forbidden!");
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn it_creates_403_response_with_json_body() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(403, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_403_response_with_json_body() {
        let response = status!(403, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn it_creates_empty_status_response_with_headers() {
        let response = status!(400; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 400);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_empty_status_response_with_body_and_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(406, payload; [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.status(), 406);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

        #[tokio::test]
    async fn it_sets_content_type_for_text_sugar() {
        let response = status!(401, "Unauthorized!");

        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_sets_content_type_for_json_typed() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(401, payload);

        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn it_sets_content_type_for_json_inline_object() {
        let response = status!(401, { "name": "test" });

        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn it_creates_text_prefix_to_string_response() {
        let response = status!(418, text: true);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 418);
    }

    #[tokio::test]
    async fn it_creates_text_prefix_number_response() {
        let response = status!(418, text: 150);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "150");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 418);
    }

    #[tokio::test]
    async fn it_creates_textf_formatted_response() {
        let name = "John";
        let response = status!(401, textf: "{} is not authorized!", name);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "John is not authorized!");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_textf_interpolated_response() {
        let name = "John";
        let response = status!(401, textf: "{name} is not authorized!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "John is not authorized!");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_textf_with_headers_using_semicolon_separator() {
        let name = "John";
        let response = status!(401, textf: "{} is not authorized!", name; [
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "John is not authorized!");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_text_prefix_with_headers_using_semicolon_separator() {
        let response = status!(401, text: true; [
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_explicit_json_mode_response() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(401, json: payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_explicit_json_string_mode_response() {
        let response = status!(401, json: "ok");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        // JSON string, not plain text
        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_json_inline_object_with_headers_using_semicolon_separator() {
        let response = status!(401, { "name": "test" }; [
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
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_empty_response_with_headers_using_semicolon_separator() {
        let response = status!(204; [
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 204);
        assert!(response.headers().get("Content-Type").is_none());
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}