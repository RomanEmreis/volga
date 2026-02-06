//! Run with:
//!
//! ```no_rust
//! cargo run -p open_api
//! ```

use volga::{App, Json, Query, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .with_open_api(|config| config
            .with_title("Open API Demo")
            .with_description("Demonstration of Open API with Volga")
            .with_version("1.0.0")
            .with_ui(true));

    app.use_open_api();

    app.map_get("/hello", async || "Hello, World!");

    app.map_get("/route/{name}", async |name: String| ok!(fmt: "Hello {name}"));
    app.map_get("/query", async |q: Query<Payload>| ok!(fmt: "Hello {}", q.name));
    app.map_post("/post", async |payload: Json<Payload>| payload);

    app.run().await
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Payload {
    name: String,
    age: u64
}