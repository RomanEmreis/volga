//! Multipart-specific error helpers.

use crate::error::Error;

pub(super) struct MultipartError;

impl MultipartError {
    #[inline]
    pub(super) fn invalid_boundary() -> Error {
        Error::client_error("Multipart error: invalid boundary")
    }

    #[inline]
    pub(super) fn missing_file_name() -> Error {
        Error::client_error("Multipart error: file name is missing")
    }

    /// The multipart could not be read
    #[inline]
    pub(super) fn read_error(error: multer::Error) -> Error {
        body_error(error)
            .unwrap_or_else(|error| Error::client_error(format!("Multipart error: {error}")))
    }

    /// A field's chunk could not be read
    #[inline]
    pub(super) fn chunk_error(error: multer::Error) -> Error {
        body_error(error)
            .unwrap_or_else(|error| Error::client_error(format!("multipart read: {error}")))
    }
}

/// The error of the body a multipart reads from, when it is the body that failed to read
///
/// It goes on as it is, with its own status and message - a `413` for a body over the limit
/// says so, where multer's "failed to read stream" would say nothing. Anything else is a
/// malformed multipart, the client's `400`.
#[inline]
fn body_error(error: multer::Error) -> Result<Error, multer::Error> {
    match error {
        multer::Error::StreamReadFailed(inner) => inner
            .downcast::<Error>()
            .map(|err| *err)
            .map_err(multer::Error::StreamReadFailed),
        error => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::StatusCode;

    #[test]
    fn it_passes_on_the_error_of_a_body_that_failed_to_read() {
        let too_large = || {
            multer::Error::StreamReadFailed(Box::new(Error::from_parts(
                StatusCode::PAYLOAD_TOO_LARGE,
                None,
                "too large",
            )))
        };

        for error in [
            MultipartError::read_error(too_large()),
            MultipartError::chunk_error(too_large()),
        ] {
            assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(error.to_string(), "too large");
        }
    }

    #[test]
    fn it_answers_400_for_a_stream_that_failed_with_an_error_of_another_kind() {
        let error = MultipartError::read_error(multer::Error::StreamReadFailed("broken".into()));

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert_eq!(error.to_string(), "Multipart error: failed to read stream");
    }

    #[test]
    fn it_answers_400_for_a_malformed_multipart() {
        let error = MultipartError::read_error(multer::Error::IncompleteStream);

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }
}
