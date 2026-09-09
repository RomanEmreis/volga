use volga::App;

struct NotAnExtractor;

fn main() {
    let mut app = App::new();
    app.map_get("/", async |_: NotAnExtractor| "hi");
}
