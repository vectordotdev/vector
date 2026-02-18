mod install_git_hooks;
mod starship;

crate::cli_subcommands! {
    "Collection of meta-utilities..."
    starship,
    install_git_hooks,
}
