//! Macros for responses with various HTTP statuses

/// Produces a response with specified `StatusCode` with plain text or JSON body
///
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::status;
///
/// status!(404);
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::status;
///
/// status!(401, "Unauthorized!");
/// ```
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
/// ```
#[macro_export]
macro_rules! status {
    ($status:expr, { $($json:tt)* }) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json($crate::json::json_internal!({ $($json)* })),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    ($status:expr) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK), 
            $crate::HttpBody::empty()
        )
    };
    
    ($status:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK), 
            $crate::HttpBody::empty(),
            [ $( ($key, $value) ),* ]
        )
    };
    
    ($status:expr, $var:ident) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json($var),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    ($status:expr, $body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK), 
            $crate::HttpBody::json($body),
            [ $( ($key, $value) ),* ]
        )
    };
    
    ($status:expr, $fmt:tt) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json(format!($fmt)),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    ($status:expr, $body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json($body),
            [
                ($crate::headers::CONTENT_TYPE, "application/json"),
            ]
        )
    };
    
    ($status:expr, $($fmt:tt)*) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
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

        assert_eq!(String::from_utf8_lossy(body), "\"You are not authorized!\"");
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn it_creates_401_response_with_interpolated_text_body() {
        let name = "John";
        let response = status!(401, "{} is not authorized!", name);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"John is not authorized!\"");
        assert_eq!(response.status(), 401);
    }
    
    #[tokio::test]
    async fn it_creates_401_response_with_formatted_text_body() {
        let name = "John";
        let response = status!(401, "{name} is not authorized!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"John is not authorized!\"");
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

        assert_eq!(String::from_utf8_lossy(body), "\"It's forbidden!\"");
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
        let response = status!(400, [
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
        let response = status!(406, payload, [
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
}