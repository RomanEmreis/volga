//! Run with:
//!
//! ```no_rust
//! cargo run --example request_validation --features middleware
//! ```

use volga::App;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Sums only positive numbers
    app
        .map_group("/positive")
        .filter(is_pos)
        .map_get("/sum/{x}/{y}", sum);
    
    // Sums only even negative numbers
    app
        .map_get("/negative/sum/{x}/{y}", sum)
        .filter(is_neg)
        .filter(is_even);

    app.run().await
}

async fn is_pos(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0
}

async fn is_neg(x: i32, y: i32) -> bool {
    x < 0 && y < 0
}

async fn is_even(x: i32, y: i32) -> Result<(), String> {
    let mut err_str = Vec::new();
    if x % 2 != 0 {
        err_str.push(format!("{x} is not even"));
    }
    if y % 2 != 0 {
        err_str.push(format!("{y} is not even"));
    }
    if !err_str.is_empty() { 
        Err(err_str.join(","))
    } else {
        Ok(())
    }
}

async fn sum(x: i32, y: i32) -> i32 {
    x + y
}