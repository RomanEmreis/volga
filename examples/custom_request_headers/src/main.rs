//! Run with:
//!
//! ```no_rust
//! cargo run -p custom_request_headers
//! ```

use volga::{
    headers::{Header, headers, http_header},
    App,
    ok
};

const CORRELATION_ID_HEADER: &str = "x-correlation-id";
const API_KEY_HEADER: &str = "x-api-key";

// Custom header if the "macros" feature is enabled
#[http_header(CORRELATION_ID_HEADER)]
struct CorrelationId;

// Define one or multiple headers if the "macros" feature is disabled
headers! {
    (ApiKey, API_KEY_HEADER),
    (SomeHeader, "x-some-header")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Setting up the "x-correlation-id" header if it's not provided
    app.wrap(|mut ctx, next| async move {
        if ctx.extract::<Header<CorrelationId>>().is_err() {
            let correlation_id = Header::<CorrelationId>::from_static("123-321-456");
            ctx.request_mut().insert_header(correlation_id);
        }
        if ctx.extract::<Header<ApiKey>>().is_err() {
            let correlation_id = Header::<ApiKey>::from_static("secret");
            ctx.request_mut().insert_header(correlation_id);
        }
        if ctx.extract::<Header<SomeHeader>>().is_err() {
            let correlation_id = Header::<SomeHeader>::from_static("some value");
            ctx.request_mut().insert_header(correlation_id);
        }
        next(ctx).await
    });

    // Reading custom header and insert it to response headers
    app.map_get("/hello", |correlation_id: Header<CorrelationId>, api_key: Header<ApiKey>, header: Header<SomeHeader>| async move {
        ok!(format!("{}: {}", header.name(), header.as_str()?), [
            (correlation_id.name(), correlation_id.as_str()?),
            (api_key.name(), api_key.as_str()?)
        ])
    });

    app.run().await
}