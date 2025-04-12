/// Produces `OK 200` response with SSE (Server-Sent Events) stream body
/// 
#[macro_export]
macro_rules! sse {
    ($body:expr) => {
        $crate::sse!(
            $body,
            []
        )
    };
    ($body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::stream!(
            $body,
            [
                ($crate::headers::CONTENT_TYPE, "text/event-stream"),
                ($crate::headers::CACHE_CONTROL, "no-cache"),
                $( ($key, $value) ),*
            ]
        )
    };
}

#[cfg(test)]
mod tests {
    use crate::{
        error::Error,
        headers::{CONTENT_TYPE, CACHE_CONTROL}
    };
    use http_body_util::BodyExt;
    use futures_util::stream::{repeat_with};
    use tokio_stream::StreamExt;
    use crate::http::sse::Message;

    #[tokio::test]
    async fn it_creates_sse_response() {
        let stream = repeat_with(|| Message::new().data("hi!"))
            .map(Ok::<_, Error>)
            .take(1);
        
        let mut response = sse!(stream).unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "data: hi!\n\n");
        assert_eq!(response.headers().get(&CONTENT_TYPE).unwrap(), "text/event-stream");
        assert_eq!(response.headers().get(&CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_creates_sse_response_with_headers() {
        let stream = repeat_with(|| "data: hi!\n\n")
            .map(Ok::<&str, Error>)
            .take(1);

        let mut response = sse!(stream, [
            ("x-header", "some value"),
        ]).unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), 200);
        assert_eq!(String::from_utf8_lossy(body), "data: hi!\n\n");
        assert_eq!(response.headers().get(&CONTENT_TYPE).unwrap(), "text/event-stream");
        assert_eq!(response.headers().get(&CACHE_CONTROL).unwrap(), "no-cache");
        assert_eq!(response.headers().get("x-header").unwrap(), "some value");
    }
}