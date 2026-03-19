The `gcp_stackdriver_logs` sink now supports extracting a custom `insertId` field from log events
via the new `insert_id_key` configuration option. The insertId is used by GCP for log de-duplication
and to order query results for logs that have the same `logName` and `timestamp` values.

authors: garethpelly
