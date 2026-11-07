#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = beamburin::import::pdf::import_pdf_report(data);
});
