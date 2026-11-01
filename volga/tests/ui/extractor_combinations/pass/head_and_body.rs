use volga::{App, Json, Query};
use std::collections::HashMap;

fn main() {
    let mut app = App::new();
    app.map_post(
        "/{id}",
        async |id: u32, _: Query<HashMap<String, String>>, _: Json<u32>| format!("{id}"),
    );
}
