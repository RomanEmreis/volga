//! Run with:
//!
//! ```no_rust
//! cargo run -p payload_validation
//! ```
//!
//! Then try:
//!
//! ```no_rust
//! curl -X POST localhost:7878/put -H "Content-Type: application/json" -d '{"key":"","value":"1"}'
//! curl "localhost:7878/items?per_page=1000000&from=5&to=1"
//! ```

use serde::Deserialize;
use volga::validation::{Validate, ValidationError};
use volga::{App, ValidJson, ValidQuery, ok};

// The derive writes out the same `validate()` a hand-written impl would, and the
// bounds it reads are published in the OpenAPI schema as well as being enforced.
#[derive(Deserialize, Validate)]
struct KeyValue {
    #[validate(length(min = 1, message = "key is required"))]
    key: String,

    #[validate(length(max = 4096))]
    value: String,
}

#[derive(Deserialize, Validate)]
// A rule spanning two fields cannot live on either of them
#[validate(schema = "from_is_before_to")]
struct Filter {
    #[validate(range(min = 1, max = 100))]
    per_page: u32,

    from: Option<u32>,
    to: Option<u32>,
}

fn from_is_before_to(filter: &Filter) -> Result<(), ValidationError> {
    if let (Some(from), Some(to)) = (filter.from, filter.to)
        && from > to
    {
        return Err(ValidationError::field("from", "must not be after `to`"));
    }
    Ok(())
}

// Volga knows nothing about the rules: it calls `validate()` while extracting the
// payload and turns the failure into a response before the handler is entered.
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Renders the validation failures as RFC 9457 problem details
    app.use_problem_details();

    app.map_post("/put", async |val: ValidJson<KeyValue>| {
        ok!("{}={}", val.key, val.value)
    });

    app.map_get("/items", async |filter: ValidQuery<Filter>| {
        ok!("page size: {}", filter.per_page)
    });

    app.run().await
}
