The `auth.load_timeout_secs` option is now applied to the STS client used to assume a role. Previously, components that sign requests directly with AWS credentials (such as the `prometheus_remote_write` sink) ignored this option and used the AWS SDK's default 3.1 second connect timeout when assuming a role.

authors: byronwolfman
