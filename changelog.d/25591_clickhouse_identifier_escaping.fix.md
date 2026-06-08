Fixed incorrect SQL identifier escaping in the `clickhouse` sink. The `database` identifier was not escaped at all, and the `table` identifier used `\"` instead of the SQL-standard `""` doubling. Both identifiers now use correct SQL-standard escaping, preventing a crafted name from breaking out of the quoted identifier.

**authors**: pront
