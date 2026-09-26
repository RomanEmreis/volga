use volga::App;

fn main() {
    let mut app = App::new();
    app.filter(|| "1".parse::<i32>().map(|_| ()));
}
