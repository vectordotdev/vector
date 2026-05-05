Added a `use_fips_endpoint` configuration option to all AWS components (sinks, sources, and secrets backends). When set to `true`, the AWS SDK resolves FIPS-compliant endpoints for the target service. Using FIPS-compliant endpoints allows per-component control over FIPS endpoint usage, which is required for compliance environments. When omitted, the existing behavior is preserved (the SDK checks the `AWS_USE_FIPS_ENDPOINT` environment variable and AWS config files).

authors: joshcoughlan
