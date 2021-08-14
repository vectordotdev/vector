use shadow_rs::SdResult;
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

    shadow_rs::new_hook(hook).unwrap();
}

fn hook(file: &File) -> SdResult<()> {
    append_build_debug(file)?;
    append_build_desc(file)?;
    Ok(())
}

fn append_build_debug(mut file: &File) -> SdResult<()> {
    let hook_const: String = format!(
        r#"/// Level of debug info for Vector.
    pub const DEBUG: &str = "{}";"#,
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
        r#"
    /// Special build description, related to versioned releases.
    pub const VECTOR_BUILD_DESC: Option<&str> = {};"#,
        build_desc
    );
    writeln!(&mut file, "{}", hook_const)?;
    Ok(())
}
