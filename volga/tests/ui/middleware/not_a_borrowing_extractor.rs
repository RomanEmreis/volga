use volga::App;

struct NotAnExtractor;

fn main() {
    let mut app = App::new();
    app.with(async |_: NotAnExtractor, next| next.await);
}
