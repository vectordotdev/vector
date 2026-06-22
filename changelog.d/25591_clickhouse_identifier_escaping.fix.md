Fixed SQL injection via identifier names in the `clickhouse` sink. The `database` and `table` config values are now passed as ClickHouse query parameters with the `Identifier` type (`{database:Identifier}.{table:Identifier}`), letting the server handle quoting rather than relying on client-side string escaping.

authors: pront thomasqueirozb
