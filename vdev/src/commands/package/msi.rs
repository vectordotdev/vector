use anyhow::Result;



#[cfg(windows)]
use {
    crate::{app, util},
    crate::app::CommandExt,
    std::ffi::{OsStr},
    std::env,
    std::fs,
    std::iter::once,
    std::path::{Path, PathBuf},
    std::process::Command,
};


// Use the `bash` interpreter included as part of the standard `git` install for our default shell
// if nothing is specified in the environment.
#[cfg(windows)]
const DEFAULT_SHELL: &str = "C:\\Program Files\\Git\\bin\\bash.EXE";
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
            // TODO: wait for other PR to be merged then replace the below line with app::version()
            let archive_version = env::var("VERSION").or_else(|_| util::read_version())?;

            // Make sure we start with a fresh `target/msi-x64` target directory and
            // copy the `distribution/msi` directory to `target/msi-x64`
            let msi_x64_dir = Path::new("target").join("msi-x64");
            fs::remove_dir_all(&msi_x64_dir).ok();
            fs::create_dir_all(&msi_x64_dir)?;
            fs::copy("distribution/msi", &msi_x64_dir)?;


            let artifacts_dir = Path::new("target").join("artifacts");
            let zip_file = format!("vector-{archive_version}-x86_64-pc-windows-msvc.zip");
            fs::copy(artifacts_dir.join(&zip_file), msi_x64_dir.join(&zip_file))?;

            // Ensure in the `msi-x64` directory
            env::set_current_dir(&msi_x64_dir)?;

            // Extract the zip file with PowerShell and build the MSI package
            let powershell_command = format!(
                "$progressPreference = 'silentlyContinue'; Expand-Archive {zip_file}"
            );
            app::exec("powershell", ["-Command", &powershell_command], false)?;
            execute_powershell_script("build.sh", once(&archive_version))?;

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
fn execute_powershell_script<T: AsRef<OsStr>>(script: &str, args: impl IntoIterator<Item = T>) -> Result<()> {
    let path: PathBuf = [app::path(), "distribution", "msi", script].into_iter().collect();
    // On Windows, all scripts must be run through an explicit interpreter.
    let mut command = Command::new(&*DEFAULT_SHELL);
    command.arg(path);

    for arg in args {
        command.arg(arg);
    }
    command.check_run()
}
