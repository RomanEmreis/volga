use volga::{App, Path};

fn main() {
    let mut app = App::new();
    app.map_get("/{a}/{b}", async |a: u32, Path(b): Path<(u32, u32)>| format!("{a}{}", b.1));
}
