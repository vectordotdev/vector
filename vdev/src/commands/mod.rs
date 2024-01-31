use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

mod compose_tests;

/// This macro simplifies the generation of CLI subcommand invocation structures by combining the
/// creation of the command enum and implementation of the dispatch function into one simple list.
#[macro_export]
macro_rules! cli_commands {
    // Peel off the list of module identifiers one-by-one
    ( :: $( $list:ident, )* :: mod $mod:ident, $( $rest:tt )* ) => {
        mod $mod;
        $crate::cli_commands! { :: $( $list, )* $mod, :: $( $rest )* }
    };
    ( :: $( $list:ident, )* :: $mod:ident, $( $rest:tt )* ) => {
        $crate::cli_commands! { :: $( $list, )* $mod, :: $( $rest )* }
    };
    // All the identifiers are parsed out, build up the enum and impl blocks
    ( :: $( $mod:ident, )* :: ) => {
        paste::paste! {
            #[derive(clap::Subcommand, Debug)]
            enum Commands {
                $( [<$mod:camel>]($mod::Cli), )*
            }

            impl Cli {
                pub fn exec(self) -> anyhow::Result<()> {
                    match self.command {
                        $( Commands::[<$mod:camel>](cli) => cli.exec(), )*
                    }
                }
            }
        }
    };
    // Start the above patterns
    ( $( $rest:tt )+ ) => { $crate::cli_commands! { :: :: $( $rest )+ } };
}

#[macro_export]
macro_rules! cli_subcommands {
    ( $doc:literal $( $rest:tt )* ) => {
        #[derive(clap::Args, Debug)]
        #[doc = $doc]
        #[command()]
        pub(super) struct Cli {
            #[command(subcommand)]
            command: Commands,
        }

        $crate::cli_commands! { $( $rest )* }
    }
}

/// Vector's unified dev tool
#[derive(Parser, Debug)]
#[command(
    version,
    bin_name = "vdev",
    infer_subcommands = true,
    disable_help_subcommand = true,
    after_help = r#"Environment variables:
  $CONTAINER_TOOL  Set the tool used to run containers (Defaults to autodetect)
                   Valid values are either "docker" or "podman".
"#
)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

cli_commands! {
    mod build,
    mod check,
    mod complete,
    mod config,
    mod crate_versions,
    mod e2e,
    mod exec,
    mod features,
    mod fmt,
    mod info,
    mod integration,
    mod meta,
    mod package,
    mod release,
    mod run,
    mod status,
    mod test,
    mod test_vrl,
    mod version,
}

/// This macro creates a wrapper for an existing script.
#[macro_export]
macro_rules! script_wrapper {
    ( $mod:ident = $doc:literal => $script:literal ) => {
        paste::paste! {
            mod $mod {
                #[doc = $doc]
                #[derive(clap::Args, Debug)]
                #[command()]
                pub(super) struct Cli {
                    args: Vec<String>,
                }

                impl Cli {
                    pub(super) fn exec(self) -> anyhow::Result<()> {
                        $crate::app::exec(concat!("scripts/", $script), self.args, true)
                    }
                }
            }
        }
    };
}
