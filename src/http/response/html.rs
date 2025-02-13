/// Produces `OK 200` response with HTML body
/// 
/// # Examples
/// ## Default usage
///```no_run
/// # use volga::HttpRequest;
/// use volga::html;
///
/// # async fn dox(request: HttpRequest) -> std::io::Result<()> {
/// html!(
///    r#"
///    <!doctype html>
///    <html>
///        <head>Hello!</head>
///        <body>
///            <p>Hello, World!</p>
///        </body>
///    </html>
///    "#);
/// # Ok(())
/// # }
#[macro_export]
macro_rules! html {
    ($body:expr) => {
        $crate::html!($body, [])
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::full($body),
            [
                ($crate::headers::CONTENT_TYPE, "text/html; charset=utf-8"),
                $( ($key, $value) ),*
            ]
        )
    };
}

#[macro_export]
macro_rules! html_file {
    ($file_name:expr, $body:expr) => {
        $crate::html_file!($file_name, $body, [])
    };
    ($file_name:expr, $body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        let mime = $crate::fs::get_mime_or_octet_stream($file_name);
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::wrap_stream($body),
            [
                ($crate::headers::CONTENT_TYPE, mime.as_ref()),
                ($crate::headers::TRANSFER_ENCODING, "chunked"),
                $( ($key, $value) ),*
            ]
        )
    }};
}

/// Produces `NO CONTENT 204` response
/// 
/// # Examples
/// ## Default usage
///```no_run
/// # use volga::HttpRequest;
/// use volga::no_content;
///
/// # async fn dox(request: HttpRequest) -> std::io::Result<()> {
/// no_content!();
/// # Ok(())
/// # } 
#[macro_export]
macro_rules! no_content {
    () => {
        $crate::response!(
            $crate::http::StatusCode::NO_CONTENT,
            $crate::HttpBody::empty()
        )
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use crate::test_utils::read_file_bytes;

    #[tokio::test]
    async fn it_creates_html_response() {
        let html_text = 
            r#"
            <!doctype html>
            <html>
                <head>Hello!</head>
                <body>
                    <p>Hello, World!</p>
                </body>
            </html>
            "#;
        
        let response = html!(html_text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(body.as_slice()), html_text);
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_html_response_with_headers() {
        let html_text =
            r#"
            <!doctype html>
            <html>
                <head>Hello!</head>
                <body>
                    <p>Hello, World!</p>
                </body>
            </html>
            "#;

        let response = html!(html_text, [
            ("x-api-key", "some api key")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(body.as_slice()), html_text);
        assert_eq!(response.headers()["x-api-key"], "some api key");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_no_content_response() {
        let response = no_content!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 204);
    }
}