use volga::{App, Path};

struct Params {
    id: u64,
}

fn main() {
    let mut app = App::new();
    app.map_get("/{id}", async |Path(p): Path<Params>| format!("{}", p.id));
}
