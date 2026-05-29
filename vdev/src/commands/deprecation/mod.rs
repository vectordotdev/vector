mod enact;
mod show;

crate::cli_subcommands! {
    "Manage and inspect deprecation notices..."
    enact,
    show,
}
