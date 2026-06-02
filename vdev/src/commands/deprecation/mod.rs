mod enact;
mod generate;
mod show;

crate::cli_subcommands! {
    "Manage and inspect deprecation notices..."
    enact,
    generate,
    show,
}
