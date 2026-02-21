Add columns configuration to PostgreSQL sink that allows users to explicitly specify which columns to insert data into. This addresses the issue where PostgreSQL's default values and serial columns are not supported, as described in issue #24461.

Key changes:
- Added columns parameter to PostgresConfig as an optional Vec<String>
- Modified PostgresService to accept and use the columns configuration
- Updated SQL query generation to use specified columns when provided
- Updated documentation warnings to mention the new columns feature
- Added test coverage for the new configuration option

Usage example:
```yaml
sinks:
  my_sink_id:
    type: postgres
    inputs:
      - my-source-or-transform-id
    endpoint: postgres://user:password@localhost/default
    table: table1
    columns:
      - column1
      - column2
```

This allows excluding columns like serial/auto-increment columns that should be handled by PostgreSQL, fixing the issue where NULL values were inserted into serial columns.

Fixes #24461
