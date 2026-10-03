//! Multipart-specific error helpers.

use crate::{error::Error, http::StatusCode};

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

    #[inline]
    pub(super) fn read_error(error: multer::Error) -> Error {
        Error::from_parts(
            read_status(&error),
            None,
            format!("Multipart error: {error}"),
        )
    }

    /// A field's chunk could not be read
    #[inline]
    pub(super) fn chunk_error(error: multer::Error) -> Error {
        Error::from_parts(
            read_status(&error),
            None,
            format!("multipart read: {error}"),
        )
    }
}

/// The status a multipart that failed to read answers with: the status of the body it reads
/// from, when it is the body that failed - a `413` for one over the body limit - and `400`
/// for a malformed multipart
#[inline]
fn read_status(error: &multer::Error) -> StatusCode {
    match error {
        multer::Error::StreamReadFailed(inner) => inner
            .downcast_ref::<Error>()
            .map_or(StatusCode::BAD_REQUEST, Error::status),
        _ => StatusCode::BAD_REQUEST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_keeps_the_status_of_a_body_that_failed_to_read() {
        let body_error = Error::from_parts(StatusCode::PAYLOAD_TOO_LARGE, None, "too large");
        let error =
            MultipartError::read_error(multer::Error::StreamReadFailed(Box::new(body_error)));

        assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn it_answers_400_for_a_malformed_multipart() {
        let error = MultipartError::read_error(multer::Error::IncompleteStream);

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }
}
