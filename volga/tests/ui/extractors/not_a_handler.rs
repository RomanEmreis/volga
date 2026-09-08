use volga::App;

fn main() {
    let mut app = App::new();
    app.map_get("/", "not a function");
}
