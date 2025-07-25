//! Run with:
//!
//! ```no_rust
//! cargo run -p response_handler
//! ```

use volga::{App, HttpResponse, headers::HeaderValue};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app
        .map_group("/positive")
        .map_ok(handler_group_response)
        .map_get("/sum/{x}/{y}", sum);

    app
        .map_get("/negative/sum/{x}/{y}", sum)
        .map_ok(handler_response);

    app.run().await
}

async fn handler_group_response(mut resp: HttpResponse) -> HttpResponse {
    resp.headers_mut().insert("x-custom-header", HeaderValue::from_static("for-group"));
    resp
}

async fn handler_response(mut resp: HttpResponse) -> HttpResponse {
    resp.headers_mut().insert("x-custom-header", HeaderValue::from_static("for-route"));
    resp
}

async fn sum(x: i32, y: i32) -> i32 {
    x + y
}