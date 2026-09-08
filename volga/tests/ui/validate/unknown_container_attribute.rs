use volga::validation::Validate;

#[derive(Validate)]
#[validate(check = "verify")]
struct Payload {
    key: String,
}

fn main() {}
