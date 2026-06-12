The `databricks_zerobus` sink now supports a `user_agent` option whose value is appended to the `user-agent` header sent to Databricks. The header always identifies Vector (`Vector/<version>`); when set, the configured value is appended after it.

authors: flaviocruz
