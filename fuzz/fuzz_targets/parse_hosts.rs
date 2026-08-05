#![no_main]

use hostctl::{DataFormat, parse_entries};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(content) = std::str::from_utf8(data) {
        let _ = parse_entries(content, DataFormat::Hosts);
    }
});
