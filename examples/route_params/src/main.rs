//! Run with:
//!
//! ```no_rust
//! cargo run -p route_params
//! ```

use std::collections::HashMap;
use serde::Deserialize;
use volga::{App, Path, ok};

#[derive(Deserialize)]
struct User {
    name: String,
    age: u32
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // GET /hello/John
    app.map_get("/hello/{name}", |name: String| async move {
        ok!("Hello {}!", name)
    });

    // GET /hello/John/33
    app.map_get("/hello/{name}/{age}", |user: Path<User>| async move {
        ok!("Hello {}! Your age is: {}", user.name, user.age)
    });

    // GET /hi/John/33
    app.map_get("/hi/{name}/{age}", |path: Path<HashMap<String, String>>| async move {
        let name = path.get("name").unwrap(); // "John"
        let age = path.get("age").unwrap(); // "33"

        ok!("Hi {}! Your age is: {}", name, age)
    });

    app.run().await
}