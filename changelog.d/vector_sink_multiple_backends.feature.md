Add support for configuring multiple endpoints in the `vector` sink via the new `routing.endpoints` option, enabling built-in `load_balance`, `failover`, and `failover_primary` endpoint strategies across downstream Vector instances. The previous `address` option is now deprecated in favor of `routing.endpoints`.

authors: fpytloun
