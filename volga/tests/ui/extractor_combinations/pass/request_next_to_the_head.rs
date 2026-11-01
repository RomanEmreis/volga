use volga::{App, HttpRequest, Path, http::Uri};

fn main() {
    let mut app = App::new();
    app.map_get("/last/{id}", async |Path(id): Path<u32>, uri: Uri, _: HttpRequest| {
        format!("{id}{uri}")
    });
    app.map_get("/first/{id}", async |_: HttpRequest, id: u32| format!("{id}"));
}
