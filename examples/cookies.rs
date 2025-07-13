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

async fn login(cookies: Cookies, auth: Header<Authorization>) -> Result<(HttpResult, Cookies), Error> {
    let session_id = authorize(auth)?;
    Ok((see_other!("/me"), cookies.add(("session-id", session_id))))
}

async fn me(cookies: Cookies) -> HttpResult {
    cookies
        .get("session-id")
        .map_or_else(
            || status!(401, "Unauthorized"),
            |_session_id| ok!("Success"))
    
}

fn authorize(_auth: Header<Authorization>) -> Result<String, Error> {
    // authorize the user and create a session
    
    Ok(Uuid::new_v4().to_string())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_post("/login", login);
    app.map_get("/me", me);

    app.run().await
}