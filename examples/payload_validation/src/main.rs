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
//! curl "localhost:7878/items?per_page=1000000"
//! ```

use serde::Deserialize;
use volga::validation::{Validate, ValidationError};
use volga::{App, ValidJson, ValidQuery, ok};

#[derive(Deserialize)]
struct KeyValue {
    key: String,
    value: String,
}

// Volga knows nothing about the rules: it calls `validate()` while extracting
// the payload and turns the failure into a response before the handler runs.
impl Validate for KeyValue {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        let mut err = ValidationError::new();
        if self.key.is_empty() {
            err.push("key", "key is required");
        }
        if self.value.len() > 4096 {
            err.push("value", "value is too long");
        }

        err.into_result()
    }
}

#[derive(Deserialize)]
struct Filter {
    per_page: u32,
    from: Option<u32>,
    to: Option<u32>,
}

impl Validate for Filter {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        let mut err = ValidationError::new();
        if self.per_page == 0 || self.per_page > 100 {
            err.push("per_page", "must be between 1 and 100");
        }
        // A cross-field rule, which is why the trait is hand-written rather than derived
        if let (Some(from), Some(to)) = (self.from, self.to)
            && from > to
        {
            err.push("from", "must not be after `to`");
        }

        err.into_result()
    }
}

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
