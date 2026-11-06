use volga::{App, HttpRequestMut, Json};

fn main() {
    let mut app = App::new();
    app.tap_req(async |req: HttpRequestMut, _: Json<u32>| req);
}
