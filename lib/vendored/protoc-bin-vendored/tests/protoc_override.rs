use std::{fs, path::PathBuf, process::Command};

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

#[test]
fn configured_protoc_finds_well_known_types() {
    // CI supplies its installed native compiler. Keep the upstream bundled-only
    // test mode available when PROTOC is not set.
    let Some(protoc) = std::env::var_os("PROTOC") else {
        return;
    };
    let directory = std::env::temp_dir().join(format!("protoc-override-{}", std::process::id()));
    fs::create_dir(&directory).expect("create proto fixture directory");
    let input = directory.join("probe.proto");
    fs::write(
        &input,
        "syntax = \"proto3\";\nimport \"google/protobuf/duration.proto\";\nmessage Probe { google.protobuf.Duration elapsed = 1; }\n",
    )
    .expect("write proto fixture");
    let output = Command::new(protoc)
        .arg("--proto_path")
        .arg(&directory)
        .arg("--descriptor_set_out")
        .arg(directory.join("probe.pb"))
        .arg(&input)
        .output()
        .expect("run configured protoc");
    fs::remove_dir_all(&directory).expect("remove proto fixture directory");
    assert!(output.status.success(), "{output:?}");
}
