mod changelog_fragments;
mod component_examples;
mod component_features;
mod deny;
mod events;
mod examples;
mod fmt;
mod generated_docs;
mod licenses;
mod markdown;
mod rust;
mod scripts;

use crate::utils::command::ScriptArgs;

/// Check parts of the Vector code base...
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    ChangelogFragments(changelog_fragments::Cli),
    GeneratedDocs(generated_docs::Cli),
    ComponentFeatures(component_features::Cli),
    ComponentExamples(component_examples::Cli),
    Deny(deny::Cli),
    /// Check that all /docs files are valid
    Docs(ScriptArgs),
    Events(events::Cli),
    Examples(examples::Cli),
    Fmt(fmt::Cli),
    Licenses(licenses::Cli),
    Markdown(markdown::Cli),
    Rust(rust::Cli),
    Scripts(scripts::Cli),
}

impl Cli {
    pub fn exec(self) -> anyhow::Result<()> {
        match self.command {
            Commands::ChangelogFragments(cli) => cli.exec(),
            Commands::GeneratedDocs(cli) => cli.exec(),
            Commands::ComponentFeatures(cli) => cli.exec(),
            Commands::ComponentExamples(cli) => cli.exec(),
            Commands::Deny(cli) => cli.exec(),
            Commands::Docs(args) => args.exec("check-docs.sh"),
            Commands::Events(cli) => cli.exec(),
            Commands::Examples(cli) => cli.exec(),
            Commands::Fmt(cli) => cli.exec(),
            Commands::Licenses(cli) => cli.exec(),
            Commands::Markdown(cli) => cli.exec(),
            Commands::Rust(cli) => cli.exec(),
            Commands::Scripts(cli) => cli.exec(),
        }
    }
}
