//! Macros for redirect responses

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
    ($url:expr) => {
        $crate::redirect!($url, [])
    };
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(301, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
        ])
    };
}

/// Produces HTTP 302 FOUND response
///
/// # Example
/// ```no_run
/// use volga::found;
///
/// let url = "https://www.rust-lang.org/";
/// found!(url);
/// ```
#[macro_export]
macro_rules! found {
    ($url:expr) => {
        $crate::found!($url, [])
    };
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(302, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
        ])
    };
}

/// Produces HTTP 303 SEE OTHER response
///
/// # Example
/// ```no_run
/// use volga::see_other;
///
/// let url = "https://www.rust-lang.org/";
/// see_other!(url);
/// ```
#[macro_export]
macro_rules! see_other {
    ($url:expr) => {
        $crate::see_other!($url, [])
    };
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(303, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
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
    ($url:expr) => {
        $crate::temp_redirect!($url, [])
    };
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(307, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
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
    ($url:expr) => {
        $crate::permanent_redirect!($url, [])
    };
    ($url:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::status!(308, [
            ($crate::headers::LOCATION, $url),
            $( ($key, $value) ),*
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

    #[tokio::test]
    async fn it_creates_found_redirect_response() {
        let url = "https://www.rust-lang.org/";
        let response = found!(url);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 302);
        assert_eq!(response.headers().get("location").unwrap(), url);
    }

    #[tokio::test]
    async fn it_creates_found_redirect_response_with_custom_headers() {
        let url = "https://www.rust-lang.org/";
        let response = found!(url, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 302);
        assert_eq!(response.headers().get("location").unwrap(), url);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn it_creates_see_other_redirect_response() {
        let url = "https://www.rust-lang.org/";
        let response = see_other!(url);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers().get("location").unwrap(), url);
    }

    #[tokio::test]
    async fn it_creates_see_other_redirect_response_with_custom_headers() {
        let url = "https://www.rust-lang.org/";
        let response = see_other!(url, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers().get("location").unwrap(), url);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}