//! Macros for SSE (Server Sent Events) responses

/// Produces `OK 200` response with SSE (Server-Sent Events) stream body
#[macro_export]
#[cfg(feature = "http2")]
macro_rules! sse {
    ($body:expr) => {
        $crate::sse!(
            $body;
            []
        )
    };
    ($body:expr; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::stream_bytes($body);
            [
                ($crate::headers::CONTENT_TYPE, "text/event-stream; charset=utf-8"),
                ($crate::headers::CACHE_CONTROL, "no-cache"),
                ($crate::headers::X_ACCEL_BUFFERING, "no"),
                $( ($key, $value) ),*
            ]
        )
    };
}

/// Produces `OK 200` response with SSE (Server-Sent Events) stream body
#[macro_export]
#[cfg(all(not(feature = "http2"), feature = "http1"))]
macro_rules! sse {
    ($body:expr) => {
        $crate::sse!(
            $body;
            []
        )
    };
    ($body:expr; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::response!(
            $crate::http::StatusCode::OK, 
            $crate::HttpBody::stream_bytes($body);
            [
                ($crate::headers::CONTENT_TYPE, "text/event-stream; charset=utf-8"),
                ($crate::headers::CACHE_CONTROL, "no-cache"),
                ($crate::headers::CONNECTION, "keep-alive"),
                ($crate::headers::X_ACCEL_BUFFERING, "no"),
                $( ($key, $value) ),*
            ]
        )
    };
}

#[cfg(test)]
mod tests {
    use crate::headers::{CONTENT_TYPE, CACHE_CONTROL, X_ACCEL_BUFFERING};
    use http_body_util::BodyExt;
    use futures_util::stream::{repeat_with, StreamExt};
    use crate::http::sse::Message;

    #[cfg(all(not(feature = "http2"), feature = "http1"))]
    use crate::headers::CONNECTION;

    #[tokio::test]
    async fn it_creates_sse_response() {
        let stream = Message::new().data("hi!")
            .repeat()
            .take(1);
        
        let mut response = sse!(stream).unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "data: hi!\n\n");
        assert_eq!(response.headers().get(&CONTENT_TYPE).unwrap(), "text/event-stream; charset=utf-8");
        assert_eq!(response.headers().get(&CACHE_CONTROL).unwrap(), "no-cache");
        #[cfg(all(not(feature = "http2"), feature = "http1"))]
        assert_eq!(response.headers().get(&CONNECTION).unwrap(), "keep-alive");
        assert_eq!(response.headers().get(&X_ACCEL_BUFFERING).unwrap(), "no");
    }

    #[tokio::test]
    async fn it_creates_sse_response_with_headers() {
        let stream = repeat_with(|| "data: hi!\n\n".into())
            .take(1);

        let mut response = sse!(stream; [
            ("x-header", "some value"),
        ]).unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "data: hi!\n\n");
        assert_eq!(response.headers().get(&CONTENT_TYPE).unwrap(), "text/event-stream; charset=utf-8");
        assert_eq!(response.headers().get(&CACHE_CONTROL).unwrap(), "no-cache");
        #[cfg(all(not(feature = "http2"), feature = "http1"))]
        assert_eq!(response.headers().get(&CONNECTION).unwrap(), "keep-alive");
        assert_eq!(response.headers().get(&X_ACCEL_BUFFERING).unwrap(), "no");
        assert_eq!(response.headers().get("x-header").unwrap(), "some value");
    }
}