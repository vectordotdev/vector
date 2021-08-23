use shadow_rs::{SdResult, Shadow};
use std::fs::File;
use std::io::Write;

fn main() {
    // Always rerun if the build script itself changes.
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(any(
        feature = "sources-vector",
        feature = "sources-dnstap",
        feature = "sinks-vector"
    ))]
    {
        println!("cargo:rerun-if-changed=proto/vector.proto");
        println!("cargo:rerun-if-changed=proto/dnstap.proto");

        let mut prost_build = prost_build::Config::new();
        prost_build.btree_map(&["."]);

        tonic_build::configure()
            .compile_with_config(
                prost_build,
                &[
                    "lib/vector-core/proto/event.proto",
                    "proto/vector.proto",
                    "proto/dnstap.proto",
                ],
                &["proto/", "lib/vector-core/proto/"],
            )
            .unwrap();
    }

    let inject_env = [
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION",
        "CARGO_PKG_DESCRIPTION",
        "TARGET",
        "CARGO_CFG_TARGET_ARCH",
        "DEBUG",
        "VECTOR_BUILD_DESC",
    ];

    let shadow = Shadow::build().unwrap();
    shadow.cargo_rerun_env_inject(&inject_env);
    shadow.hook(hook).unwrap();
}

fn hook(file: &File) -> SdResult<()> {
    append_build_debug(file)?;
    append_build_desc(file)?;
    Ok(())
}

fn append_build_debug(mut file: &File) -> SdResult<()> {
    let hook_const: String = format!(
        r###"#[doc=r#"{}"#]
    pub const DEBUG: &str = "{}";"###,
        "Level of debug info for Vector.",
        std::env::var("DEBUG")?
    );
    writeln!(&mut file, "{}", hook_const)?;
    Ok(())
}

fn append_build_desc(mut file: &File) -> SdResult<()> {
    let build_desc = std::env::var("VECTOR_BUILD_DESC")
        .map(|x| format!(r#""Some(\"{}\")""#, x))
        .unwrap_or("None".to_string());
    let hook_const: String = format!(
        r###"#[doc=r#"{}"#]
    pub const VECTOR_BUILD_DESC: Option<&str> = {};"###,
        "Special build description, related to versioned releases.", build_desc
    );
    writeln!(&mut file, "{}", hook_const)?;
    Ok(())
}
