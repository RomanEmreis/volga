//! Run with:
//!
//! ```no_rust
//! cargo run -p json
//! ```

use serde::{Deserialize, Serialize};
use volga::{App, ok, Json};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: i32
}

#[derive(Debug, Serialize, Deserialize)]
struct OptionalUser {
    name: Option<String>,
    age: Option<i32>
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Return untyped JSON
    // GET /health
    app.map_get("/health", || async {
        ok! { "status": "healthy" } // { status: "healthy" }
    });

    // Return strongly typed JSON
    // GET /user/John
    app.map_get("/user/{name}", |name: String| async move {
        let user: User = User {
            name,
            age: 35
        };
        ok!(user) // { name: "John", age: 35 }
    });

    // Read JSON body
    // POST /user
    // { name: "John", age: 35 }
    app.map_post("/user", |user: Json<User>| async move {
        user
    });

    // Read JSON body
    // POST /user
    // {}
    app.map_post("/user-optional", |user: Json<OptionalUser>| async move {
        user
    });

    app.run().await
}