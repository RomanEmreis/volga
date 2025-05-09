//! Run with:
//!
//! ```no_rust
//! cargo run --example cookies --features cookie
//! ```

use uuid::Uuid;
use volga::{
    App, HttpResult, http::Cookies,
    error::Error,
    headers::{Header, Authorization},
    status, ok, see_other
};

async fn login(mut cookies: Cookies, auth: Header<Authorization>) -> Result<(HttpResult, Cookies), Error> {
    let session_id = authorize(auth)?;
    cookies.add(("session-id", session_id));

    Ok((see_other!("/me"), cookies))
}

async fn me(cookies: Cookies) -> HttpResult {
    if let Some(_session_id) = cookies.get("session-id") { 
        ok!("Success")
    } else { 
        status!(401, "Unauthorized")
    }
}

fn authorize(_auth: Header<Authorization>) -> Result<String, Error> {
    Ok(Uuid::new_v4().to_string())
    
    // authorize the user and create a session
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_post("/login", login);
    app.map_get("/me", me);

    app.run().await
}