//! Run with:
//!
//! ```no_rust
//! cargo run -p route_params
//! ```

use serde::Deserialize;
use volga::{App, Path, NamedPath, error::Error, ok};

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
    app.map_get("/hello/{name}/{age}", |user: NamedPath<User>| async move {
        ok!("Hello {}! Your age is: {}", user.name, user.age)
    });

    // GET /hi/John/33
    app.map_get("/hi/{name}/{age}", |Path((name, age)): Path<(String, u32)>| async move {
        ok!("Hi {}! Your age is: {}", name, age)
    });

    app.map_get("/hi/{age}", |age: Option<i32>| async move {
        ok!("Age {:?}!", age)
    });

    app.map_get("/hey/{age}", |age: Result<u32, Error>| async move {
        ok!("Age {:?}!", age)
    });
    
    app.run().await
}