use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let env_cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let env_out_dir = env::var("OUT_DIR").unwrap();
    let env_path = env::var("PATH").unwrap();
    let env_target = env::var("TARGET").unwrap();
    let env_profile = env::var("PROFILE").unwrap();

    let cargo_manifest_dir = Path::new(&env_cargo_manifest_dir);
    let out_dir = Path::new(&env_out_dir);
    let precompiled_sys_lib_path = cargo_manifest_dir.join("precompiled-sys");

    for entry in precompiled_sys_lib_path.join("src").read_dir().unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    assert!(Command::new("env")
        .arg("-i")
        .arg("bash")
        .arg("-c")
        .arg(format!(
            r#"export PATH={} && TARGET={} PROFILE={} {}"#,
            env_path,
            env_target,
            env_profile,
            precompiled_sys_lib_path.join("build.sh").display(),
        ))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    let precompiled_sys_bc_path = precompiled_sys_lib_path
        .join("target")
        .join(&env_target)
        .join(&env_profile)
        .join("precompiled.bc");

    assert!(
        precompiled_sys_bc_path.exists(),
        "{} does not exist",
        precompiled_sys_bc_path.display()
    );
    assert!(out_dir.exists(), "{} does not exist", out_dir.display());

    let precompiled_sys_bc_out_path = out_dir.join("precompiled.bc");
    fs::copy(precompiled_sys_bc_path, precompiled_sys_bc_out_path).unwrap();

    if env_target.contains("darwin") {
        println!(
            "cargo:rustc-link-search={}",
            macos_link_search_path().unwrap().display()
        );
        println!("cargo:rustc-link-lib=clang_rt.osx");
    }
}

fn macos_link_search_path() -> Result<PathBuf, std::io::Error> {
    let output = Command::new("clang").arg("--print-search-dirs").output()?;
    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            if let Some(path) = line.split('=').nth(1).map(PathBuf::from) {
                return Ok(path.join("lib").join("darwin"));
            }
        }
    }

    Err(std::io::ErrorKind::NotFound.into())
}
