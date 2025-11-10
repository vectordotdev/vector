mod build;
mod ci_paths;
mod show;
mod start;
mod stop;
mod test;

crate::cli_subcommands! {
    r"Manage integration test environments...

These test setups are organized into a set of integrations, located in subdirectories
`tests/integration`.  For each integration, there is a matrix of environments, described in the
`matrix` setting in the `test.yaml` file contained in the `config/` subdirectory."

    show,
    build,
    start,
    stop,
    test,
    ci_paths,
}
