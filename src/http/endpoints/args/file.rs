//! Extractors for file stream

use bytes::Bytes;
use futures_util::future::{ok, Ready};
use http_body_util::BodyExt;
use hyper::body::Body;
use tokio::io::{AsyncWriteExt, BufWriter};

use std::{
    io::{Error, ErrorKind::{InvalidInput, InvalidData}},
    path::Path
};

use crate::http::{
    HttpBody,
    endpoints::args::{
        FromPayload,
        Payload,
        Source
    }
};
use crate::headers::CONTENT_DISPOSITION;

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
pub struct FileStream<B: Body<Data = Bytes> + Unpin> {
    name: Option<String>,
    stream: B
}

impl<B: Body<Data = Bytes> + Unpin> FileStream<B> {
    /// Create a new file stream
    fn new(name: Option<&str>, stream: B) -> Self {
        Self { 
            name: name.map(|s| s.to_string()),
            stream
        }
    }
    
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Asynchronously writes a file stream to disk with a name taken from [`CONTENT_DISPOSITION`] header
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
        
        while let Some(next) = stream.frame().await {
            match next {
                Ok(frame) => {
                    if let Some(chunk) = frame.data_ref() {
                        writer.write_all(chunk).await?
                    } else {
                        break
                    }
                },
                Err(_) => return Err(FileStreamError::read_error())
            };
        }
        writer.flush().await
    }

    #[inline]
    fn parse_file_name(content_disposition: &str) -> Option<&str> {
        let parts: Vec<&str> = content_disposition.split(';').collect();
        for part in parts {
            let part = part.trim();
            if part.starts_with("filename=") {
                let file_name = part
                    .trim_start_matches("filename=")
                    .trim_matches('"');
                return Some(file_name);
            }
        }
        None
    }
}

/// Extracts a file stream from request body
impl FromPayload for File {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Parts(parts, body) = payload {
            let name = parts.headers
                .get(&CONTENT_DISPOSITION)
                .and_then(|header| header.to_str().ok())
                .and_then(Self::parse_file_name);
            ok(FileStream::new(name, body))
        } else {
            unreachable!()
        }
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

struct FileStreamError;

impl FileStreamError {
    #[inline]
    fn read_error() -> Error {
        Error::new(InvalidInput, "File Stream error: Unable to read a file")
    }

    #[inline]
    fn missing_name() -> Error {
        Error::new(InvalidData, "File Stream error: file name is missing in the \"Content-Disposition\" header")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use hyper::Request;
    use crate::headers::CONTENT_DISPOSITION;
    use crate::http::body::HttpBody;
    use crate::http::endpoints::args::file::FileStream;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use crate::test_utils::read_file;

    #[tokio::test]
    async fn it_reads_from_payload() {
        let path = Path::new("tests/resources/test_file.txt");

        let file = tokio::fs::File::open(path).await.unwrap();
        let req = Request::get("/")
            .header(CONTENT_DISPOSITION, "attachment; filename=test_file.txt")
            .body(HttpBody::wrap_stream(file))
            .unwrap();
        
        let (parts, body) = req.into_parts();
        
        let file_stream = FileStream::from_payload(Payload::Parts(&parts, body)).await.unwrap();

        assert_eq!(file_stream.name(), Some("test_file.txt"));
        
        let path = Path::new("tests/resources/test_file_saved.txt");
        file_stream.save_as(path).await.unwrap();

        let saved_bytes = read_file(path).await;
        let content = String::from_utf8_lossy(&saved_bytes).to_string();

        assert_eq!(content, "Hello, this is some file content!");
        assert_eq!(content.len(), 33);
    }
    
    #[tokio::test]
    async fn it_saves_request_body_to_file() {
        let path = Path::new("tests/resources/test_file.txt");

        let file = tokio::fs::File::open(path).await.unwrap();
        let body = HttpBody::wrap_stream(file);

        let file_stream = FileStream::new(None, body);

        let path = Path::new("tests/resources/test_file_saved.txt");
        file_stream.save_as(path).await.unwrap();

        let saved_bytes = read_file(path).await;
        let content = String::from_utf8_lossy(&saved_bytes).to_string();

        assert_eq!(content, "Hello, this is some file content!");
        assert_eq!(content.len(), 33);
    }
}