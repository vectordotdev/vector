This directory contains a set of end-to-end test frameworks for vector which are executed by the
`vdev` tool.

These end-to-end (e2e) tests are executed with the `vdev e2e` subcommand, which behaves
identically to the `vdev integration` subcommand. See the README in the `scripts/integration`
subdirectory for more information.

The e2e tests are more of a black box test, in which we spin up a full vector instance as one
of the compose services that runs alongside the others.
