mod component_docs;
pub(crate) mod component_examples;
pub(crate) mod docs_json;
mod licenses;
mod publish_metadata;
mod vector;
mod vrl_docs;
mod vrl_wasm;

use crate::utils::command::ScriptArgs;

/// Build, generate or regenerate components...
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    ComponentDocs(component_docs::Cli),
    ComponentExamples(component_examples::Cli),
    DocsJson(docs_json::Cli),
    Licenses(licenses::Cli),
    /// Build Kubernetes manifests from latest Helm chart
    Manifests(ScriptArgs),
    PublishMetadata(publish_metadata::Cli),
    Vector(vector::Cli),
    VrlDocs(vrl_docs::Cli),
    VrlWasm(vrl_wasm::Cli),
}

impl Cli {
    pub fn exec(self) -> anyhow::Result<()> {
        match self.command {
            Commands::ComponentDocs(cli) => cli.exec(),
            Commands::ComponentExamples(cli) => cli.exec(),
            Commands::DocsJson(cli) => cli.exec(),
            Commands::Licenses(cli) => cli.exec(),
            Commands::Manifests(args) => args.exec("generate-manifests.sh"),
            Commands::PublishMetadata(cli) => cli.exec(),
            Commands::Vector(cli) => cli.exec(),
            Commands::VrlDocs(cli) => cli.exec(),
            Commands::VrlWasm(cli) => cli.exec(),
        }
    }
}
