use volga::validation::Validate;

#[derive(Validate)]
struct Payload {
    #[validate(range(message = "nope"))]
    page: u32,
}

fn main() {}
