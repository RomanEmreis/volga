use volga::{App, Path};

struct NotAPathArg;

fn main() {
    let mut app = App::new();
    app.map_get("/{id}", async |_: Path<NotAPathArg>| "hi");
}
