mod enact;
mod show;
mod sync_cue;

crate::cli_subcommands! {
    "Manage and inspect deprecation notices..."
    enact,
    show,
    sync_cue,
}
