use volga::{App, Json};

fn main() {
    let mut app = App::new();
    app.map_post("/", async |_: Json<u32>, _: Json<u32>| "hi");
}
