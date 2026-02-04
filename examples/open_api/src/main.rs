//! Run with:
//!
//! ```no_rust
//! cargo run -p open_api
//! ```

use volga::{App, Json};
use serde_json::json;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .with_open_api(|config| config
            .with_title("Open API Demo")
            .with_description("Demonstration of Open API with Volga")
            .with_version("1.0.0")
            .with_ui(true));

    app.use_open_api();

    app.map_get("/hello", async || "Hello World!");
    app.map_post("/post", async |payload: Json<Payload>| payload)
        .with_openapi(|op| op
            .with_request_example_auto_schema(json!({
                "name": "Alice",
                "age": 32
            }))
            .with_response_example_auto_schema(json!({
                "name": "Alice",
                "age": 32
            }))
        );

    app.run().await
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Payload {
    name: String,
    age: u64
}