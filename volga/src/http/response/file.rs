//! Macros for file responses

/// Produces `OK 200` response with file body
/// 
/// # Examples
/// ## Default usage
///```no_run
/// use volga::file;
/// use tokio::fs::File;
///
/// # async fn dox() -> std::io::Result<()> {
/// let file_name = "example.txt";
/// let file_data = File::open(file_name).await?;
///
/// file!(file_name, file_data);
/// # Ok(())
/// # }
/// ```
/// ## Custom headers
///```no_run
/// use volga::{file, App};
/// use tokio::fs::File;
///
/// # async fn dox() -> std::io::Result<()> {
/// let file_name = "example.txt";
/// let file_data = File::open(file_name).await?;
/// 
/// file!(file_name, file_data, [
///    ("x-api-key", "some api key")
/// ]);
/// # Ok(())   
/// # }
/// ```
#[macro_export]
macro_rules! file {
    ($file_name:expr, $body:expr) => {
        $crate::file!($file_name, $body, [])
    };
    
    ($file_name:expr, $body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        let mime = $crate::fs::get_mime_or_octet_stream($file_name);
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::file($body),
            [
                ($crate::headers::CONTENT_TYPE, mime.as_ref()),
                ($crate::headers::TRANSFER_ENCODING, "chunked"),
                ($crate::headers::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", $file_name)),
                $( ($key, $value) ),*
            ]
        )
    }};
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tokio::fs::File;
    use crate::test_utils::read_file_bytes;

    #[tokio::test]
    async fn it_creates_file_with_ok_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        let file = File::open(path).await.unwrap();

        let response = file!(file_name, file);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_file_with_ok_and_custom_headers_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();

        let file = File::open(path).await.unwrap();

        let response = file!(file_name, file, [
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