/// Produces HTTP 301 MOVED PERMANENTLY response
///
/// # Example
/// ```no_run
/// use volga::redirect;
///
/// let url = "https://www.rust-lang.org/";
/// redirect!(url);
/// ```
#[macro_export]
macro_rules! redirect {
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(301, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
        ])
    };
    ($url:expr) => {
        $crate::status!(301, [
            ($crate::headers::LOCATION, $url),
        ])
    };
}

/// Produces HTTP 307 TEMPORARY REDIRECT response
///
/// # Example
/// ```no_run
/// use volga::temp_redirect;
///
/// let url = "https://www.rust-lang.org/";
/// temp_redirect!(url);
/// ```
#[macro_export]
macro_rules! temp_redirect {
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(307, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
        ])
    };
    ($url:expr) => {
        $crate::status!(307, [
            ($crate::headers::LOCATION, $url),
        ])
    };
}

/// Produces HTTP 308 PERMANENT REDIRECT response
///
/// # Example
/// ```no_run
/// use volga::permanent_redirect;
///
/// let url = "https://www.rust-lang.org/";
/// permanent_redirect!(url);
/// ```
#[macro_export]
macro_rules! permanent_redirect {
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(308, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
        ])
    };
    ($url:expr) => {
        $crate::status!(308, [
            ($crate::headers::LOCATION, $url),
        ])
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn it_creates_redirect_response() {
        let url = "https://www.rust-lang.org/";
        let response = redirect!(url);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 301);
        assert_eq!(response.headers().get("location").unwrap(), url);
    }

    #[tokio::test]
    async fn it_creates_redirect_response_with_custom_headers() {
        let url = "https://www.rust-lang.org/";
        let response = redirect!(url, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 301);
        assert_eq!(response.headers().get("location").unwrap(), url);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_temporary_redirect_response() {
        let url = "https://www.rust-lang.org/";
        let response = temp_redirect!(url);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 307);
        assert_eq!(response.headers().get("location").unwrap(), url);
    }

    #[tokio::test]
    async fn it_creates_redirect_temporary_response_with_custom_headers() {
        let url = "https://www.rust-lang.org/";
        let response = temp_redirect!(url, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 307);
        assert_eq!(response.headers().get("location").unwrap(), url);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_permanent_redirect_response() {
        let url = "https://www.rust-lang.org/";
        let response = permanent_redirect!(url);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 308);
        assert_eq!(response.headers().get("location").unwrap(), url);
    }

    #[tokio::test]
    async fn it_creates_permanent_redirect_response_with_custom_headers() {
        let url = "https://www.rust-lang.org/";
        let response = permanent_redirect!(url, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 308);
        assert_eq!(response.headers().get("location").unwrap(), url);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}