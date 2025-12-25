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
    ({ $($json:tt)* }) => {
        $crate::status!(404, { $($json)* })
    };
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(404, [ $( ($key, $value) ),* ])
    };
    ($var:ident) => {
        $crate::status!(404, $var)
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(404, $body, [ $( ($key, $value) ),* ])
    };
    ($fmt:tt) => {
        $crate::status!(404, $fmt)
    };
    ($body:expr) => {
        $crate::status!(404, $body)
    };
    ($($fmt:tt)*) => {
        $crate::status!(404, $($fmt)*)
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
    ({ $($json:tt)* }) => {
        $crate::status!(400, { $($json)* })
    };
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(400, [ $( ($key, $value) ),* ])
    };
    ($var:ident) => {
        $crate::status!(400, $var)
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(400, $body, [ $( ($key, $value) ),* ])
    };
    ($fmt:tt) => {
        $crate::status!(400, $fmt)
    };
    ($body:expr) => {
        $crate::status!(400, $body)
    };
    ($($fmt:tt)*) => {
        $crate::status!(400, $($fmt)*)
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
    ({ $($json:tt)* }) => {
        $crate::status!(201, { $($json)* })
    };
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(201, [ $( ($key, $value) ),* ])
    };
    ($var:ident) => {
        $crate::status!(201, $var)
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(201, $body, [ $( ($key, $value) ),* ])
    };
    ($fmt:tt) => {
        $crate::status!(201, $fmt)
    };
    ($body:expr) => {
        $crate::status!(201, $body)
    };
    ($($fmt:tt)*) => {
        $crate::status!(201, $($fmt)*)
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
    ({ $($json:tt)* }) => {
        $crate::status!(202, { $($json)* })
    };
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(202, [ $( ($key, $value) ),* ])
    };
    ($var:ident) => {
        $crate::status!(202, $var)
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(202, $body, [ $( ($key, $value) ),* ])
    };
    ($fmt:tt) => {
        $crate::status!(202, $fmt)
    };
    ($body:expr) => {
        $crate::status!(202, $body)
    };
    ($($fmt:tt)*) => {
        $crate::status!(202, $($fmt)*)
    };
}

/// Creates HTTP Request/Response headers
/// # Examples
///```no_run
///use volga::headers;
///
///let headers = headers![
///    ("header 1", "value 1"),
///    ("header 2", "value 2"),
///];
/// ```
#[macro_export]
macro_rules! headers {
    ( $( ($key:expr, $value:expr) ),* $(,)? ) => {{
        let mut headers = std::collections::HashMap::new();
        $(
            headers.insert($key.to_string(), $value.to_string());
        )*
        headers
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

        assert_eq!(String::from_utf8_lossy(body), "\"Error: test\"");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_creates_400_with_formatted_text_response() {
        let text = "test";
        let response = bad_request!("Error: {}", text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"Error: test\"");
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
        let response = bad_request!(payload, [
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

        assert_eq!(String::from_utf8_lossy(body), "\"User not found\"");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_interpolated_text() {
        let user = "User";
        let response = not_found!("{user} not found");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"User not found\"");
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_creates_404_response_with_formatted_text() {
        let user = "User";
        let response = not_found!("{} not found", user);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"User not found\"");
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
        let response = not_found!(payload, [
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

        assert_eq!(String::from_utf8_lossy(body), "\"User created\"");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_interpolated_text() {
        let user = "User";
        let response = created!("{user} created");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"User created\"");
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn it_creates_201_response_with_formatted_text() {
        let user = "User";
        let response = created!("{} created", user);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"User created\"");
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
        let response = created!(payload, [
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

        assert_eq!(String::from_utf8_lossy(body), "\"Task accepted\"");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_interpolated_text() {
        let task = "Task";
        let response = accepted!("{task} accepted");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"Task accepted\"");
        assert_eq!(response.status(), 202);
    }

    #[tokio::test]
    async fn it_creates_202_response_with_formatted_text() {
        let task = "Task";
        let response = accepted!("{} accepted", task);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"Task accepted\"");
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
        let response = accepted!(payload, [
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
    async fn it_creates_headers() {
        let headers = headers![
            ("header 1", "value 1"),
            ("header 2", "value 2")
        ];
        
        assert_eq!(headers.get("header 1").unwrap(), "value 1");
        assert_eq!(headers.get("header 2").unwrap(), "value 2")
    }
}