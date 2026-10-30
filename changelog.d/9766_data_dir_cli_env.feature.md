Added a `--data-dir` command-line flag (and corresponding `VECTOR_DATA_DIR` environment variable) to override the `data_dir` global configuration option. When set, it takes precedence over any `data_dir` value in the configuration file. This is useful for keeping deployment-specific paths out of the configuration file, for example when validating a configuration in a CI environment where the configured `data_dir` may not exist.

authors: xfocus3
