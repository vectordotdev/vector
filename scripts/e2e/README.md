This directory contains a set of end-to-end test frameworks for vector which are executed by the
`vdev` tool.

Currently these e2e tests are executed with the same `vdev` subcommand as the integration tests,
`cargo vdev integration`.

See the README in the `scripts/integration` subdirectory for more information.

A pending future enhancement is to create a `vdev` subcommand `e2e`, that will separate the
invocation of the end-to-end tests from the integration tests in `vdev`, to correspond to the
code separation and fundamental differences between the two classes of tests.

See https://github.com/vectordotdev/vector/issues/18829 for more information.

For now, any subdir here needs to be unique from the other integration tests outside this dir.
For example there is already a `datadog-logs` integration test, hence the e2e test is in a sub-
dir called `e2e-datadog-logs`.
