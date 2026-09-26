//! Run with:
//!
//! ```no_rust
//! cargo run -p custom_error
//! ```

use serde::Serialize;
use std::num::ParseIntError;
use volga::{
    App, Json,
    error::{Error, IntoError},
    http::StatusCode,
};

/// The errors this API answers with
enum ApiError {
    /// No item has this id
    NotFound(u32),
    /// The id is not a number
    BadId(ParseIntError),
}

/// Lets `?` turn a parse failure into an `ApiError`
impl From<ParseIntError> for ApiError {
    fn from(err: ParseIntError) -> Self {
        Self::BadId(err)
    }
}

/// The body every `ApiError` answers with
#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

/// Makes `ApiError` the `Err` of a handler's `Result`
impl IntoError for ApiError {
    fn into_error(self) -> Error {
        let (status, code, message) = match self {
            ApiError::NotFound(id) => (StatusCode::NOT_FOUND, "not_found", format!("no item {id}")),
            ApiError::BadId(err) => (StatusCode::BAD_REQUEST, "bad_id", err.to_string()),
        };
        let body = Json(ErrorBody {
            code,
            message: message.clone(),
        });

        // The status and message are what `map_err` reads; the body is what the client gets
        Error::from_parts(status, None, message).with_response(body)
    }
}

fn get_item(id: String) -> Result<Json<u32>, ApiError> {
    let id: u32 = id.parse()?;
    if id > 100 {
        return Err(ApiError::NotFound(id));
    }
    Ok(Json(id))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // GET /items/7   -> 200 7
    // GET /items/abc -> 400 {"code":"bad_id","message":"invalid digit found in string"}
    // GET /items/500 -> 404 {"code":"not_found","message":"no item 500"}
    app.map_get("/items/{id}", get_item);

    // A status code or a message is an error as it is:
    // GET /teapot -> 418 I'm a teapot
    // GET /broken -> 500 something went wrong
    app.map_get("/teapot", || Err::<(), _>(StatusCode::IM_A_TEAPOT));
    app.map_get("/broken", || Err::<(), _>("something went wrong"));

    // Every one of them comes through here; returning the error answers as it would have
    app.map_err(|error: Error| {
        eprintln!("{} {error}", error.status());
        error
    });

    app.run().await
}
