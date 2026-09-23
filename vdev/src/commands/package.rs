use crate::utils::command::ScriptArgs;

/// Package Vector in various formats...
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Create a .tar.gz package for the specified $TARGET
    Archive(ScriptArgs),
    /// Create a .deb package to be distributed in the APT package manager
    Deb(ScriptArgs),
    /// Create a .msi package for Windows
    Msi(ScriptArgs),
    /// Create a .rpm package to be distributed in the YUM package manager
    Rpm(ScriptArgs),
}

impl Cli {
    pub fn exec(self) -> anyhow::Result<()> {
        match self.command {
            Commands::Archive(args) => args.exec("package-archive.sh"),
            Commands::Deb(args) => args.exec("package-deb.sh"),
            Commands::Msi(args) => args.exec("package-msi.sh"),
            Commands::Rpm(args) => args.exec("package-rpm.sh"),
        }
    }
}
