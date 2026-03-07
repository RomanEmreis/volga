#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_LEN: usize = 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }

    let query = String::from_utf8_lossy(data);
    volga::fuzzing::fuzz_query_decode(&query);
});
