//! Run with:
//!
//! ```no_rust
//! cargo run --example cookies --features cookie,basic-auth
//! ```

use uuid::Uuid;
use volga::{
    App, HttpResult, http::Cookies,
    auth::Basic,
    error::Error,
    status, ok, see_other
};

async fn login(cookies: Cookies, auth: Basic) -> Result<(HttpResult, Cookies), Error> {
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

fn authorize(auth: Basic) -> Result<String, Error> {
    if auth.validate("foo", "bar") {
        Ok(Uuid::new_v4().to_string())    
    } else { 
        Err(Error::client_error("Invalid Credentials"))
    } 
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_post("/login", login);
    app.map_get("/me", me);

    app.run().await
}