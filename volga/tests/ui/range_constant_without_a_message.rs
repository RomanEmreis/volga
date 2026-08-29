use volga::validation::Validate;

const MIN: u32 = 1;

#[derive(Validate)]
struct Payload {
    #[validate(range(min = MIN))]
    page: u32,
}

fn main() {}
