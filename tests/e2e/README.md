This directory contains a set of end-to-end test frameworks for vector which are executed by the
`vdev` tool.

Each directory contains:

1. A `config/` subdirectory with:
   - A `compose.yaml` file containing the instructions to `docker compose` or `podman compose` for how
     to set up the containers in which to run the tests
   - A `test.yaml` file that describes how to run the end-to-end tests, including a matrix of
     software versions or other parameters over which the tests will be run
2. A `data/` subdirectory (optional) containing test data files, configuration files, and other
   resources needed by the test

You can list these tests with `cargo vdev e2e show`, which provides a list of all the
end-to-end test names followed by the extrapolated matrix of environments.

Each test can be run using one of the following:

1. Run a single test environment from the above list with `cargo vdev e2e test NAME ENV`
2. Run all the environments for one test with `cargo vdev e2e test NAME`
3. Run all the steps individually using the `start`, `test`, and then `stop` subcommands with the
   same parameters as above (see below). This allows developers to start the environment once and
   then repeat the testing step while working on a component.

```shell
cargo vdev e2e start NAME ENVIRONMENT
cargo vdev e2e test NAME [ENVIRONMENT]
cargo vdev e2e stop NAME [ENVIRONMENT]
```

If no environment is named for the `test` and `stop` subcommands, all active environments are used.

## E2E vs Integration Tests

The end-to-end (E2E) tests are black box tests that spin up a full Vector instance as one of the
Docker Compose services, running alongside external systems (e.g., Datadog Agent, Splunk, OTEL
collectors). This differs from integration tests, which compile and run Vector within a test runner
container to test individual components or integrations in isolation.
