The deprecated `--strict-env-vars` flag has been removed. has been changed to `true`. The previous
behavior of defaulting unset environment variables can be accomplished by syntax like `${FOO-}`
(which will default `FOO` to empty string if unset). See the [configuration environment variables
docs](https://vector.dev/docs/reference/configuration/#environment-variables) for more about this
syntax.
