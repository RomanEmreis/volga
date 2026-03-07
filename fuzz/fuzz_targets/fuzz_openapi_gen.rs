#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_LEN: usize = 256;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > MAX_LEN {
        return;
    }

    let selector = u16::from_le_bytes([data[0], *data.get(1).unwrap_or(&0)]);
    volga::fuzzing::fuzz_openapi_gen(selector);
});
