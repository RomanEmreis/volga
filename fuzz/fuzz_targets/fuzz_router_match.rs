#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use hyper::Method;
use libfuzzer_sys::fuzz_target;

const MAX_LEN: usize = 512;

#[inline]
fn truncate_utf8_safe(s: &mut String, max_len: usize) {
    if s.len() <= max_len {
        return;
    }

    let mut idx = max_len;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    s.truncate(idx);
}

#[derive(Debug)]
struct RouterInput {
    method: Method,
    path: String,
    host: Option<String>,
}

impl<'a> Arbitrary<'a> for RouterInput {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let method = match u.int_in_range::<u8>(0..=8)? {
            0 => Method::GET,
            1 => Method::POST,
            2 => Method::PUT,
            3 => Method::DELETE,
            4 => Method::PATCH,
            5 => Method::OPTIONS,
            6 => Method::HEAD,
            7 => Method::TRACE,
            _ => Method::CONNECT,
        };

        let path_len = u.int_in_range::<usize>(0..=MAX_LEN)?;
        let mut path = String::from_utf8_lossy(u.bytes(path_len)?).into_owned();
        if !path.starts_with('/') {
            path.insert(0, '/');
        }
        truncate_utf8_safe(&mut path, MAX_LEN);

        let host = if u.arbitrary::<bool>()? {
            let len = u.int_in_range::<usize>(0..=64)?;
            let mut h = String::from_utf8_lossy(u.bytes(len)?).into_owned();
            h.retain(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
            if h.is_empty() {
                None
            } else {
                Some(h)
            }
        } else {
            None
        };

        Ok(Self { method, path, host })
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = RouterInput::arbitrary(&mut Unstructured::new(data)) else {
        return;
    };

    volga::fuzzing::fuzz_router_match(input.method, &input.path, input.host.as_deref());
});
