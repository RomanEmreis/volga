use volga::{App, error::Error, http::endpoints::args::{FromPathArg, PathArg}};

struct OrderId(u64);

impl FromPathArg for OrderId {
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        arg.parse().map(OrderId)
    }
}

fn main() {
    let mut app = App::new();
    app.map_get("/orders/{id}", async |id: OrderId| format!("{}", id.0));
}
