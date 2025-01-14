/// Produces `OK 200` response with Form Data body
///
/// # Example
/// ```no_run
/// use std::collections::HashMap;
/// use volga::form;
///
/// # async fn dox() -> std::io::Result<()> {
/// let data = HashMap::from([
///     ("key", "value")
/// ]);
///
/// form!(data);
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! form {
    // handles form!({ "key": "value" })
    ({ $($json:tt)* }) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::form($crate::json::json_internal!({ $($json)* })),
            [
                ($crate::headers::CONTENT_TYPE, "application/x-www-form-urlencoded"),
            ]
        )
    };
    
    // handles form!({ "key": "value" }, [("key", "val")])
    ({ $($json:tt)* }, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::form($crate::json::json_internal!({ $($json)* })),
            [
                ($crate::headers::CONTENT_TYPE, "application/x-www-form-urlencoded"),
                $( ($key, $value) ),*
            ]
        )
    };
    
    // handles form!(object, [("key", "val")])
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::form($body),
            [
                ($crate::headers::CONTENT_TYPE, "application/x-www-form-urlencoded"),
                $( ($key, $value) ),*
            ]
        )
    };
    
    // handles form!(object)
    ($body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::form($body),
            [
                ($crate::headers::CONTENT_TYPE, "application/x-www-form-urlencoded"),
            ]
        )
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use std::collections::HashMap;

    #[tokio::test]
    async fn it_creates_form_data_response() {
        let data = HashMap::from([
            ("key", "value"),
        ]);
        let response = form!(data);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "key=value");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/x-www-form-urlencoded");
    }

    #[tokio::test]
    async fn it_creates_form_data_response_with_headers() {
        let data = HashMap::from([
            ("key", "value"),
        ]);
        let response = form!(data, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "key=value");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/x-www-form-urlencoded");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_form_data_untyped_response() {
        let response = form!({ "key": "value" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "key=value");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/x-www-form-urlencoded");
    }

    #[tokio::test]
    async fn it_creates_form_data_untyped_response_with_headers() {
        let response = form!({ "key": "value" }, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "key=value");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/x-www-form-urlencoded");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}