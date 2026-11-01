use volga::{App, HttpRequest, Path};

fn main() {
    let mut app = App::new();
    app.map_get("/{id}", async |_: HttpRequest, Path(id): Path<u32>| format!("{id}"));
}
