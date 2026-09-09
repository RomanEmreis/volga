use volga::App;

struct NotAResponse;

fn main() {
    let mut app = App::new();
    app.map_get("/", async || NotAResponse);
}
