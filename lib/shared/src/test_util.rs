use crate::trace;
use std::env;

pub fn trace_init() {
    #[cfg(unix)]
    let color = atty::is(atty::Stream::Stdout);
    // Windows: ANSI colors are not supported by cmd.exe
    // Color is false for everything except unix.
    #[cfg(not(unix))]
    let color = false;

    let levels = env::var("TEST_LOG").unwrap_or_else(|_| "off".to_string());

    trace::init(color, false, &levels);
}
