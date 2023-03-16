use anyhow::Result;

#[cfg(windows)]
use {
    crate::app,
    crate::app::CommandExt as _,
    crate::util,
    std::env,
    std::fs::File,
    std::io::Write,
    std::fs,
    std::path::{Path},
    std::process::Command,
};

/// Create a .msi package for Windows
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        #[cfg(not(windows))]
        {
            println!("Sorry, the package-msi command is not supported on non-Windows platforms. Nothing was performed.");
        }
        #[cfg(windows)]
        {
            let archive_version = app::version()?;

            let package_version = {
                let channel = util::release_channel()?;
                if channel == "custom" {
                    // Extract the version before ".custom"
                    archive_version.split(".custom").next().unwrap().to_string()
                } else {
                    archive_version.clone()
                }
            };
            // Make sure we start with a fresh `target/msi-x64` target directory and
            // copy the `distribution/msi` directory to `target/msi-x64`
            let msi_x64_dir = Path::new("target").join("msi-x64");
            fs::remove_dir_all(&msi_x64_dir).ok();
            fs::create_dir_all(&msi_x64_dir)?;
            fs::copy("distribution/msi", &msi_x64_dir)?;


            let artifacts_dir = Path::new("target").join("artifacts");
            let zip_file = format!("vector-{archive_version}-x86_64-pc-windows-msvc.zip");
            fs::copy(artifacts_dir.join(&zip_file), msi_x64_dir.join(&zip_file))?;

            // Ensure we are in the `msi-x64` directory
            env::set_current_dir(&msi_x64_dir)?;

            // Extract the zip file with PowerShell and build the MSI package
            let powershell_command = format!(
                "$progressPreference = 'silentlyContinue'; Expand-Archive {zip_file}"
            );
            app::exec("powershell", ["-Command", &powershell_command], false)?;
            build(&archive_version, &package_version)?;

            // Change the current directory back to the original path
            env::set_current_dir(app::path())?;

            // Copy the MSI file to the artifacts directory
            let msi_file = format!("vector-{archive_version}-x64.msi");
            let dest_file = artifacts_dir.join(msi_file);
            fs::copy(msi_x64_dir.join("vector.msi"), dest_file)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn build(archive_version: &str, package_version: &str) -> Result<()> {
    println!("Running Build with args: {archive_version}");
    println!("Copying ZIP archive...");

    println!("Preparing LICENSE.rtf..");
    let mut license_rtf_file = File::create("LICENSE.rtf")?;
    writeln!(
        license_rtf_file,
        "{{\\rtf1\\ansi\\ansicpg1252\\deff0\\nouicompat{{\\fonttbl{{\\f0\\fnil\\fcharset0 Lucida Console;}}}}\n\\viewkind4\\uc1\n\\pard\\f0\\fs14\\lang1033\\par"
    )?;

    let license_content_path = format!("vector-{archive_version}-x86_64-pc-windows-msvc/LICENSE.txt");
    let license_content = std::fs::read_to_string(license_content_path)?;
    for line in license_content.lines() {
        writeln!(license_rtf_file, "{line}\\")?;
    }
    writeln!(license_rtf_file, "\n}}")?;

    println!("Substituting version...");
    let vector_tmpl = std::fs::read_to_string("vector.wxs.tmpl")?;
    let vector_tmpl_updated = vector_tmpl.replace("${VERSION}", package_version);
    let mut vector_wxs_file = File::create("vector.wxs")?;
    writeln!(vector_wxs_file, "{vector_tmpl_updated}")?;

    println!("Building the MSI package...");
    let vector_dir = format!("vector-{archive_version}-x86_64-pc-windows-msvc");
    let args = &[
        &format!("dir {vector_dir}"),
        "-cg Vector",
        "-dr INSTALLDIR",
        "-gg",
        "-sfrag",
        "-srd",
        "-var var.VectorDir",
        "-out components.wxs"
    ];
    Command::new("heat").args(args).capture_output()?;

    // Add Win64="yes" to Component elements
    // See https://stackoverflow.com/questions/22932942/wix-heat-exe-win64-components-win64-yes
    let components_text = std::fs::read_to_string("components.wxs")?;
    let components_text = components_text.replace("<Component ", r#"<Component Win64="yes" "#);
    let mut components_file = File::create("components.wxs")?;
    write!(components_file, "{components_text}")?;

    // Call WiX toolset to build MSI package
    let binding = &format!("-dVectorDir={vector_dir}");
    let mut args = vec![
        "candle",
        "components.wxs",
        binding
    ];
    Command::new("candle").args(&args).capture_output()?;

    args = vec![
        "candle",
        "vector.wxs",
        "-ext",
        "WiXUtilExtension",
    ];
    Command::new("candle").args(&args).capture_output()?;

    args = vec![
        "vector.wixobj",
        "components.wixobj",
        "-out",
        "vector.msi",
        "-ext",
        "WixUIExtension",
        "-ext",
        "WiXUtilExtension",
    ];
    Command::new("light").args(&args).capture_output()?;
    Ok(())
}
