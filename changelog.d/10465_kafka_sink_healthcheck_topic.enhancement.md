Added a new `healthcheck_topic` configuration option to the `kafka` sink. Previously, `topic` was used to perform healthchecks, but a templated value would cause healthchecks to fail.

authors: yalinglee
