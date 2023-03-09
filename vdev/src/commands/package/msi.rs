use anyhow::Result;

#[cfg(windows)]
use {
    crate::{app, util},
    std::env,
    std::fs,
    std::iter::once,
    std::path::Path,
};

/// Create a .msi package for Windows
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        #[cfg(windows)]
        {
            // TODO: wait for other PR to be merged then replace the below line with app::version()
            let archive_version = env::var("VERSION").or_else(|_| util::read_version())?;

            // rm -rf target/msi-x64
            // cp -R distribution/msi target/msi-x64
            let msi_x64_dir = Path::new("target").join("msi-x64");
            fs::remove_dir_all(&msi_x64_dir).ok();
            fs::create_dir_all(&msi_x64_dir)?;
            fs::copy("distribution/msi", &msi_x64_dir)?;

            // cp target/artifacts/vector-"${ARCHIVE_VERSION}"-x86_64-pc-windows-msvc.zip target/msi-x64
            let artifacts_dir = Path::new("target").join("artifacts");
            let zip_file = format!("vector-{archive_version}-x86_64-pc-windows-msvc.zip");
            fs::copy(artifacts_dir.join(&zip_file), msi_x64_dir.join(&zip_file))?;

            // pushd target/msi-x64
            env::set_current_dir(&msi_x64_dir)?;

            // powershell '$progressPreference = "silentlyContinue"; Expand-Archive vector-'"$ARCHIVE_VERSION"'-x86_64-pc-windows-msvc.zip'
            let powershell_command = format!(
                "$progressPreference = 'silentlyContinue'; Expand-Archive vector-{zip_file}"
            );
            app::exec("powershell", ["-Command", &powershell_command], false)?;

            // ./build.sh "${ARCHIVE_VERSION}"
            app::exec("build.sh", once(&archive_version), true)?;

            // popd
            env::set_current_dir(app::path())?;

            // cp target/msi-x64/vector.msi target/artifacts/vector-"${ARCHIVE_VERSION}"-x64.msi
            let msi_file = format!("vector-{archive_version}-x64.msi");
            let dest_file = artifacts_dir.join(msi_file);
            fs::copy(msi_x64_dir.join("vector.msi"), dest_file)?;
        }
        Ok(())
    }
}
