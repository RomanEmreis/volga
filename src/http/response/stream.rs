/// Produces `OK 200` response with stream body
/// 
/// # Examples
/// ## Default usage
///```no_run
/// use volga::{HttpRequest, stream};
///
/// # async fn dox(request: HttpRequest) -> std::io::Result<()> {
/// let boxed_body = request.into_boxed_body();
/// stream!(boxed_body);
/// # Ok(())
/// # }
/// ```
/// ## Custom headers
///```no_run
/// use volga::{HttpRequest, stream};
///
/// # async fn dox(request: HttpRequest) -> std::io::Result<()> {
/// let boxed_body = request.into_boxed_body();
/// stream!(boxed_body, [
///    ("Content-Type", "message/http")
/// ]);
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! stream {
    ($body:expr) => {
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::new($body)
        )
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::new($body),
            [ $( ($key, $value) ),* ]
        )
    };
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tokio::fs::File;
    use crate::HttpBody;
    use crate::test_utils::read_file_bytes;

    #[tokio::test]
    async fn it_creates_stream_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file = File::open(path).await.unwrap();
        let body = HttpBody::wrap_stream(file);

        let response = stream!(body.into_boxed());

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_stream_response_with_custom_headers() {
        let path = Path::new("tests/resources/test_file.txt");
        let file = File::open(path).await.unwrap();
        let body = HttpBody::wrap_stream(file);

        let response = stream!(body.into_boxed(), [
            ("x-api-key", "some api key")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
        assert_eq!(response.headers()["x-api-key"], "some api key");
        assert_eq!(response.status(), 200);
    }
}