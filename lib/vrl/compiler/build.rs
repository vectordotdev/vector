use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/precompiled/src/lib.rs");

    let precompiled_path = PathBuf::from(format!(
        "{}/src/precompiled/target/{}/{}/precompiled.bc",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
        env::var("TARGET").unwrap(),
        env::var("PROFILE").unwrap()
    ));

    if precompiled_path.exists() {
        fs::copy(
            precompiled_path,
            format!("{}/precompiled.bc", env::var("OUT_DIR").unwrap()),
        )
        .unwrap();
    }
}
