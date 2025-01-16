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
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(404, [ $( ($key, $value) ),* ])
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
    ({ $($json:tt)* }) => {
        $crate::status!(400, { $($json)* })
    };
    ([ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(400, [ $( ($key, $value) ),* ])
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(400, $body, [ $( ($key, $value) ),* ])
    };
    ($body:expr) => {
        $crate::status!(400, $body)
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
        let mut headers = $crate::HttpHeaders::new();
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
    async fn it_creates_headers() {
        let headers = headers![
            ("header 1", "value 1"),
            ("header 2", "value 2")
        ];
        
        assert_eq!(headers.get("header 1").unwrap(), "value 1");
        assert_eq!(headers.get("header 2").unwrap(), "value 2")
    }
}