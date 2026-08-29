use volga::validation::Validate;

#[derive(Validate)]
struct Payload {
    #[validate(length(equal = 3, min = 5))]
    key: String,
}

fn main() {}
