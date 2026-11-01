use volga::{App, Json};

fn main() {
    let mut app = App::new();
    app.with(async |_: Json<u32>, next| next.await);
}
