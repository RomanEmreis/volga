use volga::{App, blocking, ok};

fn main() {
    let mut app = App::new();
    app.map_get("/", blocking(async || ok!()));
}
