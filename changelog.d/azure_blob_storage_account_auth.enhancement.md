Enhance the `azure_blob` sink with a `storage_account` authentication mode in addition to `connection_string`, including support for Azure identity-based auth strategies (`default`, `environment`, `managed_identity`, `azure_cli`, and `workload_identity`) and optional custom endpoint configuration.

Existing `connection_string` configurations continue to work unchanged. Exactly one of `connection_string` or `storage_account` must be set.

When `auth` is not set, the sink defaults to `default` strategy and attempts credentials in order: environment (including workload identity), managed identity, then Azure CLI.

authors: rjancewicz
