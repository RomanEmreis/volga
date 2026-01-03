//! Run with:
//!
//! ```no_rust
//! cargo run -p route_groups
//! ```

use volga::{App, HttpResult, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.group("/user", |group| {
        group.map_get("/{id}", get_user);
        group.map_post("/create/{name}", create_user); 
    });

    app.run().await
}

async fn get_user(_id: i32) -> &'static str {
    "John"
}

async fn create_user(name: String) -> HttpResult {
    ok!("User {name} created!")
}