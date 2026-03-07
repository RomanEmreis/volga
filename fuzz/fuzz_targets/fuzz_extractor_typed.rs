#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;

const MAX_BODY: usize = 4096;
const MAX_HEADERS: usize = 16;

#[derive(Arbitrary, Debug)]
struct ExtractorInput {
    headers: Vec<(Vec<u8>, Vec<u8>)>,
    body: Vec<u8>,
}

fuzz_target!(|data: &[u8]| {
    let Ok(mut input) = ExtractorInput::arbitrary(&mut Unstructured::new(data)) else {
        return;
    };

    if input.body.len() > MAX_BODY {
        input.body.truncate(MAX_BODY);
    }

    if input.headers.len() > MAX_HEADERS {
        input.headers.truncate(MAX_HEADERS);
    }

    let headers = input
        .headers
        .into_iter()
        .map(|(k, v)| {
            let mut key = String::from_utf8_lossy(&k).into_owned();
            key.retain(|c| c.is_ascii_alphanumeric() || c == '-');
            let val = String::from_utf8_lossy(&v).into_owned();
            (key, val)
        })
        .collect::<Vec<_>>();

    volga::fuzzing::fuzz_extractor_typed(&headers, &input.body);
});
