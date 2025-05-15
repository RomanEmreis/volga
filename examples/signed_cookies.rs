//! Run with:
//!
//! ```no_rust
//! cargo run --example signed_cookies --features cookie,signed-cookie,di
//! ```

use uuid::Uuid;
use volga::{
    App, HttpResult, 
    error::Error,
    http::{SignedCookies, SignedKey},
    headers::{Header, Authorization},
    status, ok, see_other
};

async fn login(cookies: SignedCookies, auth: Header<Authorization>) -> Result<(HttpResult, SignedCookies), Error> {
    let session_id = authorize(auth)?;
    Ok((see_other!("/me"), cookies.add(("session-id", session_id))))
}

async fn me(cookies: SignedCookies) -> HttpResult {
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

    app.add_singleton(SignedKey::generate());
    
    app.map_post("/login", login);
    app.map_get("/me", me);

    app.run().await
}