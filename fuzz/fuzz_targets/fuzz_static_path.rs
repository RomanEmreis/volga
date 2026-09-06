#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_LEN: usize = 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }

    let input = String::from_utf8_lossy(data);
    // The first line is the prefix the mount answers under, the rest is the request target.
    let (prefix, path) = match input.split_once('\n') {
        Some((prefix, path)) => (prefix, path),
        None => ("", input.as_ref()),
    };

    volga::fuzzing::fuzz_static_path(path, prefix);
});
