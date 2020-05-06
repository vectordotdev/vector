#![cfg(all(feature = "sources", feature = "sinks-console"))]

extern crate assert_cmd;

use assert_cmd::prelude::*;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use vector::test_util::temp_file;

// TODO: Test
//  - ShutdownSignal
//  - GracefullShutdown
//  - All sources closed shutdown

const STDIO_CONFIG: &'static str = r#"
[sources.in]
    type = "stdin"

[sinks.out]
    inputs = ["in"]
    type = "console"
    encoding = "text"
"#;

/// Creates a file with given content
fn create_file(config: &str) -> PathBuf {
    let path = temp_file();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path.clone())
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.flush().unwrap();

    path
}

#[test]
fn auto_shutdown() {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c").arg(create_file(STDIO_CONFIG)).arg("--quiet");

    // Once `stdin source` reads whole buffer it will automatically
    // shutdown which will also cause vector process to shutdown
    // because all sources have shutdown.
    let assert = cmd.with_stdin().buffer("42").assert();

    assert.success().stdout("42\n");
}
