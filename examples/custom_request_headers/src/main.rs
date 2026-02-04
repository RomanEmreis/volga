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
        let req = ctx.request_mut();

        req.insert_header(CorrelationId::from_static("123-321-456"));
        req.insert_header(ApiKey::from_static("secret"));
        req.insert_header(SomeHeader::from_static("some value"));

        next(ctx).await
    });

    // Reading custom header and insert it to response headers
    app.map_get("/hello", |correlation_id: Header<CorrelationId>, api_key: Header<ApiKey>, header: Header<SomeHeader>| async move {
        ok!(format!("{}: {}", header.name(), header.as_str()?); [
            correlation_id,
            api_key
        ])
    });

    app.run().await
}