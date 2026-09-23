use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

mod compose_tests;

/// This macro simplifies the generation of CLI subcommand invocation structures by combining the
/// creation of the command enum and implementation of the dispatch function into one simple list.
// Module declaration in here was removed due to https://github.com/rust-lang/rustfmt/issues/3253
#[macro_export]
macro_rules! cli_commands {
    ( :: $( $list:ident, )* :: $mod:ident, $( $rest:tt )* ) => {
        $crate::cli_commands! { :: $( $list, )* $mod, :: $( $rest )* }
    };
    // All the identifiers are parsed out, build up the enum and impl blocks
    ( :: $( $mod:ident, )* :: ) => {
        pastey::paste! {
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

mod build;
pub(crate) mod changelog;
mod check;
mod complete;
mod crate_versions;
mod deprecation;
mod e2e;
mod features;
mod fmt;
mod info;
mod integration;
mod package;
mod release;
mod run;
mod status;
mod style;
mod test;
mod test_vrl;
mod version;

cli_commands! {
    build,
    changelog,
    check,
    complete,
    crate_versions,
    deprecation,
    e2e,
    features,
    fmt,
    info,
    integration,
    package,
    release,
    run,
    status,
    test,
    test_vrl,
    version,
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory as _, error::ErrorKind};

    use super::Cli;

    const SCRIPT_COMMANDS: &[(&str, &str)] = &[
        ("build", "manifests"),
        ("check", "docs"),
        ("package", "archive"),
        ("package", "deb"),
        ("package", "msi"),
        ("package", "rpm"),
        ("release", "docker"),
        ("release", "s3"),
    ];

    #[test]
    fn script_commands_forward_arguments() {
        let cases: &[(&[&str], &[&str])] = &[
            (&[], &[]),
            (&["0.58.0"], &["0.58.0"]),
            (
                &["--chart-version", "0.46.0"],
                &["--chart-version", "0.46.0"],
            ),
            (&["--chart-version=0.46.0"], &["--chart-version=0.46.0"]),
            (
                &["-x", "path with spaces", "-1"],
                &["-x", "path with spaces", "-1"],
            ),
            (&["value", "--help", "-v"], &["value", "--help", "-v"]),
            (&["--", "--help"], &["--help"]),
            (
                &["--", "--chart-version", "0.46.0"],
                &["--chart-version", "0.46.0"],
            ),
        ];

        for &(group, command) in SCRIPT_COMMANDS {
            for &(args, expected) in cases {
                let matches = Cli::command()
                    .try_get_matches_from(
                        ["vdev", group, command]
                            .into_iter()
                            .chain(args.iter().copied()),
                    )
                    .unwrap_or_else(|error| panic!("{group} {command} {args:?}: {error}"));
                let script = matches
                    .subcommand_matches(group)
                    .unwrap()
                    .subcommand_matches(command)
                    .unwrap();
                let forwarded: Vec<_> = script
                    .get_many::<String>("args")
                    .into_iter()
                    .flatten()
                    .map(String::as_str)
                    .collect();
                assert_eq!(forwarded, expected, "{group} {command} {args:?}");
            }
        }
    }

    #[test]
    fn script_commands_keep_vdev_help() {
        for &(group, command) in SCRIPT_COMMANDS {
            let error = Cli::command()
                .try_get_matches_from(["vdev", group, command, "--help"])
                .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::DisplayHelp, "{group} {command}");
        }
    }
}
