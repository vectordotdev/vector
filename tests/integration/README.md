This directory contains a set of integration test frameworks for vector which are executed by the
`vdev` tool.

Each directory contains two files:

1. A `compose.yaml` file containing the instructions to `docker compose` or `podman compose` for how
   to set up the containers in which to run the integrations, and
2. A `test.yaml` file that describes how to run the integration tests, including a matrix of
   software versions or other parameters over which the tests will be run.

You can list these tests with `cargo vdev integration show`[1], which provides a list of all the
integration test names followed by the extrapolated matrix of environments.

Each test can be run using one of the following:

1. Run a single test environment from the above list with `cargo vdev integration test NAME ENV`
2. Run all the environments for one test with `cargo vdev integration test NAME`
3. Run all the steps individually using the `start`, `test`, and then `stop` subcommands with the
   same parameters as above (see below). This allows developers to start the environment once and
   then repeat the testing step while working on a component.

```shell
cargo vdev integration start NAME ENVIRONMENT
cargo vdev integration test NAME [ENVIRONMENT]
cargo vdev integration stop NAME [ENVIRONMENT]
```

If no environment is named for the `test` and `stop` subcommands, all active environments are used.

[1] Note that the `vdev` tool accepts abbreviated subcommand names, so this can also be run as
`cargo vdev int show` for brevity.
