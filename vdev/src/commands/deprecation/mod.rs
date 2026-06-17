mod check;
mod enact;
mod generate;
mod show;

crate::cli_subcommands! {
    "Manage and inspect deprecation notices..."
    check,
    enact,
    generate,
    show,
}
