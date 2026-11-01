use volga::{App, HttpRequest, Json};

fn main() {
    let mut app = App::new();
    app.map_post("/", async |_: HttpRequest, _: Json<u32>| "hi");
}
