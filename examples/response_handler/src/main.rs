//! Run with:
//!
//! ```no_rust
//! cargo run -p response_handler
//! ```

use volga::{App, HttpResponse, headers::custom_headers};

custom_headers! {
    (CustomHeader, "x-custom-header")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.group("/positive", |api| {
        api.map_ok(handler_group_response);
        api.map_get("/sum/{x}/{y}", sum);        
    });

    app
        .map_get("/negative/sum/{x}/{y}", sum)
        .map_ok(handler_response);

    app.run().await
}

async fn handler_group_response(mut resp: HttpResponse) -> HttpResponse {
    resp.try_insert_header::<CustomHeader>("for-group").unwrap();
    resp
}

async fn handler_response(mut resp: HttpResponse) -> HttpResponse {
    resp.try_insert_header::<CustomHeader>("for-route").unwrap();
    resp
}

async fn sum(x: i32, y: i32) -> i32 {
    x + y
}