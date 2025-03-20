//! Run with:
//!
//! ```no_rust
//! cargo run --example form
//! ```

use serde::{Deserialize, Serialize};
use volga::{App, Form, form};

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

    // Return Form Data
    // GET /health
    app.map_get("/health", || async {
        form!({ "status": "healthy" }) // status=healthy
    });

    // Return strongly typed Form Data
    // GET /user/John
    app.map_get("/user/{name}", |name: String| async move {
        let user: User = User {
            name,
            age: 35
        };
        form!(user) // name=John&age=35
    });
    
    // Read Form Data
    // POST /user
    // name=John&age=35
    app.map_post("/user", |user: Form<User>| async move {
        user
    });

    // Read Form Data
    // POST /user
    // name=John
    app.map_post("/user-optional", |user: Form<OptionalUser>| async move {
        user
    });
    
    app.run().await
}
