The ClickHouse sink's ArrowStream format now correctly handles MATERIALIZED, ALIAS, EPHEMERAL, and DEFAULT columns. MATERIALIZED, ALIAS, and EPHEMERAL columns are excluded from the fetched schema since they cannot receive INSERT data. DEFAULT columns are kept but marked nullable so events are not rejected when the server-computed value is omitted.

authors: benjamin-awd
