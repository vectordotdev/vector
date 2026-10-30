Added `extra_query_params` configuration field to the ClickHouse sink, allowing arbitrary ClickHouse session settings to be forwarded as HTTP query parameters on every INSERT request (e.g. `deduplicate_blocks_in_dependent_materialized_views`, `insert_quorum`, `max_insert_block_size`).

authors: valerypetrov
