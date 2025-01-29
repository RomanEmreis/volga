﻿//! Extractors for the whole HTTP request

use crate::{error::Error, HttpRequest};
use futures_util::future::{ok, Ready};

use crate::http::endpoints::args::{
    FromPayload,
    Payload,
    Source
};

impl FromPayload for HttpRequest {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Full(req) = payload {
            ok(req)
        } else {
            unreachable!()
        }
    }

    fn source() -> Source {
        Source::Full
    }
}