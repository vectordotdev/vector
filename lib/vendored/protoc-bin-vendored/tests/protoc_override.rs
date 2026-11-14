use std::{path::PathBuf, process::Command};

#[test]
fn configured_protoc_takes_precedence() {
    // No executable needs to exist here: this tests path selection, not protoc.
    // A child process keeps the environment override separate from other tests.
    let expected = PathBuf::from("native protoc").join("bin").join("protoc");
    let output = Command::new(env!("CARGO_BIN_EXE_protoc-bin-which"))
        .env("PROTOC", &expected)
        .output()
        .expect("run protoc-bin-which");

    assert!(output.status.success(), "{output:?}");
    let selected = String::from_utf8(output.stdout).expect("UTF-8 path output");
    assert_eq!(selected.trim_end(), expected.to_str().expect("UTF-8 path"));
}
