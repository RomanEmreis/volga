use volga::{App, HttpRequest};

fn main() {
    let mut app = App::new();
    app.map_get("/{id}", async |_: HttpRequest, id: u32| format!("{id}"));
}
