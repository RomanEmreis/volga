//! Extractors for file stream

use crate::{
    ByteStream, error::Error, fs::file_name_in_folder, headers::CONTENT_DISPOSITION,
    headers::content_disposition::parse_filename,
};
use bytes::Bytes;
use futures_util::future::{Ready, ok};
use futures_util::{TryFutureExt, TryStreamExt};
use http_body_util::BodyExt;
use hyper::body::Body;
use std::{borrow::Cow, path::Path};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::http::{
    HttpBody,
    endpoints::args::{FromPayload, Payload, Source},
};

/// See [`FileStream<B>`] for more details.
pub type File = FileStream<HttpBody>;

/// Describes a single file stream
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, File, ok};
///
/// async fn handle(file: File) -> HttpResult {
///     file.save_as("example.txt").await?;        
///     ok!("File saved!")
/// }
/// ```
pub struct FileStream<B: Body<Data = Bytes, Error = Error> + Unpin> {
    name: Option<String>,
    stream: B,
}

impl<B: Body<Data = Bytes, Error = Error> + Unpin> std::fmt::Debug for FileStream<B> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileStream(..)")
    }
}

impl<B: Body<Data = Bytes, Error = Error> + Unpin> FileStream<B> {
    /// Create a new file stream
    fn new(name: Option<String>, stream: B) -> Self {
        Self { name, stream }
    }

    /// Returns the file name from the request's [`CONTENT_DISPOSITION`] header: `filename*`
    /// when it is there, `filename` otherwise, unquoted.
    ///
    /// It is the client's name, as sent: it may hold `/`, `\` or `..`. [`save`](Self::save)
    /// keeps it inside its folder; a path built from it by hand has to do the same.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Asynchronously writes a file stream to disk with a name taken from [`CONTENT_DISPOSITION`] header
    ///
    /// The file goes into the `path` folder whatever the name: only what follows its last
    /// `/` or `\` is used, so `../../etc/passwd` is saved as `passwd`. A name with nothing
    /// usable left, such as `..` or one ending in a separator, is answered with
    /// `400 Bad Request`, as a missing one is.
    ///
    /// # Example
    /// ```no_run
    /// # use volga::{HttpResult, ok};
    /// use volga::File;
    ///
    /// # async fn handle(file: File) -> HttpResult {
    /// file.save("path/to/folder").await?;        
    /// # ok!("File saved!")
    /// # }
    /// ```
    #[inline]
    pub async fn save(self, path: impl AsRef<Path>) -> Result<(), Error> {
        let file_name = self.name().ok_or(FileStreamError::missing_name())?;
        let file_name = file_name_in_folder(file_name).ok_or(FileStreamError::invalid_name())?;
        let file_path = path.as_ref().join(file_name);

        self.save_as(file_path).await
    }

    /// Asynchronously writes a file stream to disk with a provided name in `file_path`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::{HttpResult, ok};
    /// use volga::File;
    ///
    /// # async fn handle(file: File) -> HttpResult {
    /// file.save_as("path/to/file.txt").await?;        
    /// # ok!("File saved!")
    /// # }
    /// ```
    #[inline]
    pub async fn save_as(self, file_path: impl AsRef<Path>) -> Result<(), Error> {
        let file = tokio::fs::File::create(file_path).await?;

        let mut writer = BufWriter::new(file);
        let mut stream = self.stream;

        // A body that fails to read - a `413` for one over the body limit - fails with its
        // own status and message
        while let Some(frame) = stream.frame().await {
            let Ok(chunk) = frame?.into_data() else {
                break;
            };
            writer.write_all(&chunk).await?
        }
        writer.flush().map_err(FileStreamError::flush_error).await
    }

    /// Consumes file stream into raw bytes stream
    pub fn into_stream(self) -> impl futures_util::Stream<Item = Result<Bytes, Error>> {
        self.stream.into_data_stream().into_stream()
    }

    /// Consumes file into [`ByteStream`]
    pub fn into_byte_stream(
        self,
    ) -> ByteStream<impl futures_util::Stream<Item = Result<Bytes, Error>>> {
        ByteStream::new(self.into_stream())
    }
}

/// Extracts a file stream from the request body
impl FromPayload for File {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Full;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Full(parts, body) = payload else {
            unreachable!()
        };

        let name = parts
            .headers
            .get(&CONTENT_DISPOSITION)
            .and_then(|header| std::str::from_utf8(header.as_bytes()).ok())
            .and_then(parse_filename)
            .map(Cow::into_owned);

        ok(FileStream::new(name, body))
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config.consumes_stream()
    }
}

struct FileStreamError;

impl FileStreamError {
    #[inline]
    fn flush_error(error: std::io::Error) -> Error {
        Error::client_error(format!("File Stream error: {error}"))
    }

    #[inline]
    fn missing_name() -> Error {
        Error::client_error(
            "File Stream error: file name is missing in the \"Content-Disposition\" header",
        )
    }

    #[inline]
    fn invalid_name() -> Error {
        Error::client_error("File Stream error: the file name is not a valid file name")
    }
}

#[cfg(test)]
mod tests {
    use crate::headers::CONTENT_DISPOSITION;
    use crate::http::body::HttpBody;
    use crate::http::endpoints::args::file::FileStream;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use crate::test::{TempFile, utils::read_file};
    use hyper::Request;
    use std::path::Path;
    use uuid::Uuid;

    #[tokio::test]
    async fn it_reads_from_payload() {
        let file = TempFile::new("Hello, this is some file content!").await;

        let file = tokio::fs::File::open(file.path).await.unwrap();
        let req = Request::get("/")
            .header(CONTENT_DISPOSITION, "attachment; filename=test_file.txt")
            .body(HttpBody::file(file))
            .unwrap();

        let (parts, body) = req.into_parts();

        let file_stream = FileStream::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        assert_eq!(file_stream.name(), Some("test_file.txt"));

        let path = Path::new("tests").join(format!("test_file_{}.txt", Uuid::new_v4()));
        file_stream.save_as(&path).await.unwrap();

        let saved_bytes = read_file(&path).await;
        let content = String::from_utf8_lossy(&saved_bytes);

        assert_eq!(content, "Hello, this is some file content!");
        assert_eq!(content.len(), 33);

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn it_saves_request_body_to_file() {
        let file = TempFile::new("Hello, this is some file content!").await;

        let file = tokio::fs::File::open(file.path).await.unwrap();
        let body = HttpBody::file(file);

        let file_stream = FileStream::new(None, body);

        let path = Path::new("tests").join(format!("test_file_{}.txt", Uuid::new_v4()));
        file_stream.save_as(&path).await.unwrap();

        let saved_bytes = read_file(&path).await;
        let content = String::from_utf8_lossy(&saved_bytes);

        assert_eq!(content, "Hello, this is some file content!");
        assert_eq!(content.len(), 33);

        tokio::fs::remove_file(path).await.unwrap();
    }

    fn request_with_name(content_disposition: &str) -> hyper::http::request::Parts {
        let (parts, _) = Request::get("/")
            .header(CONTENT_DISPOSITION, content_disposition)
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    async fn file_stream(content_disposition: &str) -> FileStream<HttpBody> {
        let parts = request_with_name(content_disposition);
        FileStream::from_payload(Payload::Full(&parts, HttpBody::full("content")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn it_reads_an_escaped_or_ext_name() {
        let stream = file_stream(r#"attachment; filename="a\"b.txt""#).await;
        assert_eq!(stream.name(), Some("a\"b.txt"));

        let stream = file_stream(
            "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf",
        )
        .await;
        assert_eq!(stream.name(), Some("résumé.pdf"));

        let stream = file_stream("attachment; filename=\"résumé.pdf\"").await;
        assert_eq!(stream.name(), Some("résumé.pdf"));
    }

    #[tokio::test]
    async fn it_saves_inside_the_folder_whatever_the_name() {
        let outer = tempfile::tempdir().unwrap();
        let inner = outer.path().join("inner");
        tokio::fs::create_dir(&inner).await.unwrap();

        for name in ["../escape.txt", "/escape.txt", "..\\escape.txt"] {
            let header = format!("attachment; filename=\"{}\"", name.replace('\\', "\\\\"));
            file_stream(&header).await.save(&inner).await.unwrap();

            assert!(inner.join("escape.txt").exists(), "{name}");
            assert!(!outer.path().join("escape.txt").exists(), "{name}");
            tokio::fs::remove_file(inner.join("escape.txt"))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn it_refuses_a_name_with_nothing_usable_left() {
        let dir = tempfile::tempdir().unwrap();

        for name in ["..", ".", "a/", ""] {
            let header = format!("attachment; filename=\"{name}\"");
            let err = file_stream(&header)
                .await
                .save(dir.path())
                .await
                .unwrap_err();
            assert_eq!(err.status(), 400, "{name}");
        }

        let err = FileStream::new(None, HttpBody::full("x"))
            .save(dir.path())
            .await
            .unwrap_err();
        assert_eq!(err.status(), 400);
    }
}
