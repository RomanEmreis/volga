//! Extractors for multipart/form data

use bytes::Bytes;
use crate::error::Error;
use crate::headers::{HeaderMap, CONTENT_TYPE};
use futures_util::future::{ready, Ready};
use tokio::io::{AsyncWriteExt, BufWriter};

use std::{
    ops::{Deref, DerefMut},
    path::Path
};

use crate::http::endpoints::args::{
    FromPayload,
    Payload,
    Source
};

/// Describes a multipart file/form data
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Multipart, ok};
///
/// async fn handle(multipart: Multipart) -> HttpResult {
///     multipart.save_all("path/to/folder").await?;
///     ok!("Files saved!")
/// }
/// ```
#[derive(Debug)]
pub struct Multipart(multer::Multipart<'static>);

/// Represents a single field in a multipart stream
/// 
///> See also [`multer::Field`]
#[derive(Debug)]
pub struct Field(multer::Field<'static>);

impl Field {
    /// Tries to read a file name, if it's not present tries to read a field name, otherwise returns [`Error`]
    #[inline]
    pub fn try_get_file_name(&self) -> Result<&str, Error> {
        self.0.file_name()
            .or(self.name())
            .ok_or(MultipartError::missing_file_name())
    }

    /// Get the full field data as text.
    /// 
    ///> See also [`multer::Field::text`]
    #[inline]
    pub async fn text(self) -> Result<String, Error> {
        self.0.text()
            .await
            .map_err(MultipartError::read_error)
    }

    /// Stream a chunk of the field data.
    ///
    /// When the field data has been exhausted, this will return [`None`].
    ///
    ///> See also [`multer::Field::chunk`]
    #[inline]
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error> {
        self.0.chunk()
            .await
            .map_err(MultipartError::read_error)
    }

    /// Asynchronously writes a multipart field as a file stream to disk with a name taken from [`CONTENT_DISPOSITION`] header
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpResult, Multipart, ok};
    ///
    /// async fn handle(mut files: Multipart) -> HttpResult {
    ///     while let Some(field) = files.next_field().await? {
    ///         field.save("path/to/folder").await?;
    ///     }        
    ///     ok!("File saved!")
    /// }
    /// ```
    #[inline]
    pub async fn save(self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let file_name = self.try_get_file_name()?;
        let file_path = path.as_ref().join(file_name);
        
        self.save_as(file_path).await
    }

    /// Asynchronously writes a multipart field as a file stream to disk with a provided name in `file_path`
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpResult, Multipart, ok};
    /// use std::path::Path;
    ///
    /// async fn handle(mut files: Multipart) -> HttpResult {
    ///     let path = Path::new("path/to/folder");
    ///     let mut counter = 0;
    ///     while let Some(field) = files.next_field().await? {
    ///         field.save_as(path.join(format!("file_{counter}.dat"))).await?;
    ///         counter += 1;
    ///     }        
    ///     ok!("File saved!")
    /// }
    /// ```
    #[inline]
    pub async fn save_as(mut self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let file = tokio::fs::File::create(path).await?;
        let mut writer = BufWriter::new(file);
        while let Some(ref chunk) = self.chunk().await? {
            writer.write_all(chunk).await?
        }
        writer.flush().await
    }
}

impl Deref for Field {
    type Target = multer::Field<'static>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Field {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Multipart {
    /// Asynchronously writes a multipart files to disk
    /// # Example
    /// ```no_run
    /// # use volga::{HttpResult, ok};
    /// use volga::Multipart;
    ///
    /// # async fn handle(files: Multipart) -> HttpResult {
    /// files.save_all("path/to/folder").await?;        
    /// # ok!("File saved!")
    /// # }
    /// ```
    pub async fn save_all(mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        while let Some(field) = self.next_field().await? {
            field.save(&path).await?;
        }
        Ok(())
    }
    
    /// Yields the next [`Field`] if available
    #[inline]
    pub async fn next_field(&mut self) -> Result<Option<Field>, Error> {
        self.0.next_field().await
            .map_err(MultipartError::read_error)
            .map(|field| field.map(Field))
    }
    
    #[inline]
    fn parse_boundary(headers: &HeaderMap) -> Option<String> {
        let content_type = headers.get(CONTENT_TYPE)?.to_str().ok()?;
        multer::parse_boundary(content_type).ok()
    }
}

impl Deref for Multipart {
    type Target = multer::Multipart<'static>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Multipart {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> TryFrom<Payload<'a>> for Multipart {
    type Error = Error;
    
    #[inline]
    fn try_from(payload: Payload<'a>) -> Result<Self, Self::Error> {
        let Payload::Full(parts, body) = payload else { unreachable!() };

        let boundary = Self::parse_boundary(&parts.headers)
            .ok_or(MultipartError::invalid_boundary())?;

        let stream = body.into_data_stream();
        let multipart = multer::Multipart::new(stream, boundary);

        Ok(Multipart(multipart))
    }
}

/// Extracts a file stream from the request body
impl FromPayload for Multipart {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Full;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        ready(payload.try_into())
    }
}

struct MultipartError;
impl MultipartError {
    #[inline]
    fn invalid_boundary() -> Error {
        Error::client_error("Multipart error: invalid boundary")
    }

    #[inline]
    fn missing_file_name() -> Error {
        Error::client_error("Multipart error: file name is missing")
    }

    #[inline]
    fn read_error(error: multer::Error) -> Error {
        Error::client_error(format!("Multipart error: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Multipart;
    use hyper::Request;
    use crate::headers::CONTENT_TYPE;
    use crate::http::body::HttpBody;
    use crate::http::endpoints::args::{FromPayload, Payload};
    
    #[tokio::test]
    async fn it_reads_from_payload() {
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mut multipart = Multipart::from_payload(Payload::Full(&parts, body)).await.unwrap();

        while let Some(field) = multipart.next_field().await.unwrap() {
            assert_eq!(field.name().unwrap(), "my_text_field");
            assert_eq!(field.text().await.unwrap(), "abcd");
        }
    }

    #[tokio::test]
    async fn it_reads_file_name() {
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mut multipart = Multipart::from_payload(Payload::Full(&parts, body)).await.unwrap();

        while let Some(field) = multipart.next_field().await.unwrap() {
            assert_eq!(field.try_get_file_name().unwrap(), "my_text_field");
        }
    }
    
    fn create_multipart_req() -> Request<HttpBody> {
        let data = "--X-BOUNDARY\r\nContent-Disposition: form-data; name=\"my_text_field\"\r\n\r\nabcd\r\n--X-BOUNDARY--\r\n";

        Request::get("/")
            .header(CONTENT_TYPE, "multipart/form-data; boundary=X-BOUNDARY")
            .body(HttpBody::full(data))
            .unwrap()
    }
}