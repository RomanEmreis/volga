use volga::validation::Validate;

#[derive(Validate)]
enum Payload {
    Key(String),
}

fn main() {}
