use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// This macro simplifies the generation of CLI subcommand invocation structures by combining the
/// creation of the command enum and implementation of the dispatch function into one simple list.
#[macro_export]
macro_rules! cli_commands {
    ( $( $mod:ident ),* ) => { $crate::cli_commands! { $( $mod, )* } };
    ( $( $mod:ident, )* ) => {
        $( mod $mod; )*

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
    }
}

#[macro_export]
macro_rules! cli_subcommands {
    ( $doc:literal $( $mod:ident ),* ) => {
        #[derive(clap::Args, Debug)]
        #[doc = $doc]
        #[command()]
        pub(super) struct Cli {
            #[command(subcommand)]
            command: Commands,
        }

        $crate::cli_commands! { $( $mod, )* }
    }
}

/// Vector's unified dev tool
#[derive(Parser, Debug)]
#[command(
    version,
    bin_name = "vdev",
    infer_subcommands = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

cli_commands! {
    build,
    complete,
    config,
    exec,
    integration,
    meta,
    status,
    test,
}
