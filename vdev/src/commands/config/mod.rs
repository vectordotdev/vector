mod find;
mod set;

crate::cli_subcommands! {
    "Manage the vdev config file..."
    find,
    set,
}
