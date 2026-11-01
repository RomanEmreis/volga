use volga::{App, HttpRequest, http::Uri};

fn main() {
    let mut app = App::new();
    app.map_get("/", async |_: HttpRequest, uri: Uri| format!("{uri}"));
}
