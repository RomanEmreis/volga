use volga::validation::Validate;

#[derive(Validate)]
struct Payload {
    #[validate(email)]
    key: String,
}

fn main() {}
