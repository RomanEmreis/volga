use volga::App;

struct NotAnError;

fn main() {
    let mut app = App::new();
    app.map_get("/", async || Err::<&'static str, _>(NotAnError));
}
