use volga::validation::Validate;

#[derive(Validate)]
struct Payload {
    #[validate(length(message = "nope"))]
    key: String,
}

fn main() {}
