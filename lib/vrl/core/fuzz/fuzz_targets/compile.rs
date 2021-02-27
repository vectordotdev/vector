#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|s: &str| {
    let _ = vrl::compile(s, &vrl_stdlib::all());
});
