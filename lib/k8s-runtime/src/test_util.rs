#![cfg(test)]

pub fn trace_init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_log() {
    trace_init();
    info!("Log works");
}
