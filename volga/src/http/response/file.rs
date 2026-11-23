//! Macros for file responses

/// Produces `OK 200` response with file body
///
/// The file name sets `Content-Type`, guessed from its extension, and
/// `Content-Disposition: attachment`, built by [`ContentDisposition::attachment`]: the name
/// may come from user input, since it cannot add parameters to the header, and a non-ASCII
/// name reaches the client intact through `filename*`.
///
/// [`ContentDisposition::attachment`]: crate::headers::ContentDisposition::attachment
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
/// file!(file_name, file_data; [
///    ("x-api-key", "some api key")
/// ]);
/// # Ok(())   
/// # }
/// ```
#[macro_export]
macro_rules! file {
    ($file_name:expr, $body:expr) => {
        $crate::file!($file_name, $body; [])
    };

    ($file_name:expr, $body:expr; [ $( $header:expr),* $(,)? ]) => {{
        let file_name = $file_name;
        let mime = $crate::fs::get_mime_or_octet_stream(&file_name);
        $crate::response!(
            $crate::http::StatusCode::OK,
            $crate::HttpBody::file($body);
            [
                ($crate::headers::CONTENT_TYPE, mime.as_ref()),
                $crate::headers::ContentDisposition::attachment(
                    ::core::convert::AsRef::<str>::as_ref(&file_name)
                ),
                $( $header ),*
            ]
        )
    }};
}

#[cfg(test)]
mod tests {
    use crate::test::{TempFile, utils::read_file_bytes};
    use tokio::fs::File;

    #[tokio::test]
    async fn it_creates_file_with_ok_response() {
        let file = TempFile::new("Hello, this is some file content!").await;
        let file_name = file.file_name().to_string();
        let file = File::open(file.path).await.unwrap();

        let response = file!(&file_name, file);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(
            String::from_utf8_lossy(body.as_slice()),
            "Hello, this is some file content!"
        );
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_creates_file_with_ok_and_custom_headers_response() {
        let file = TempFile::new("Hello, this is some file content!").await;
        let file_name = file.file_name().to_string();
        let file = File::open(file.path).await.unwrap();

        let response = file!(&file_name, file; [
            ("x-api-key", "some api key")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;

        assert_eq!(
            String::from_utf8_lossy(body.as_slice()),
            "Hello, this is some file content!"
        );
        assert_eq!(response.headers()["x-api-key"], "some api key");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_keeps_a_hostile_file_name_inside_the_disposition() {
        let file = TempFile::new("x").await;
        let file = File::open(file.path).await.unwrap();

        let response = file!("a\"; filename=\"evil.exe", file).unwrap();

        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=\"a\\\"; filename=\\\"evil.exe\"; \
             filename*=UTF-8''a%22%3B%20filename%3D%22evil.exe"
        );
    }

    #[tokio::test]
    async fn it_sends_a_non_ascii_file_name_as_an_ext_value() {
        let file = TempFile::new("x").await;
        let file = File::open(file.path).await.unwrap();

        let response = file!("résumé.pdf", file).unwrap();

        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"
        );
        assert_eq!(response.headers()["content-type"], "application/pdf");
    }

    #[tokio::test]
    async fn it_answers_a_file_name_with_cr_lf() {
        let file = TempFile::new("x").await;
        let file = File::open(file.path).await.unwrap();

        let response = file!("a\r\nb.txt", file).unwrap();

        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=\"a__b.txt\""
        );
    }

    #[tokio::test]
    async fn it_takes_an_owned_file_name() {
        let file = TempFile::new("x").await;
        let file = File::open(file.path).await.unwrap();
        let file_name = String::from("report.txt");

        let response = file!(file_name, file).unwrap();

        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=\"report.txt\""
        );
        assert_eq!(response.headers()["content-type"], "text/plain");
    }
}
