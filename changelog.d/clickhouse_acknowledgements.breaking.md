Updated the end status for end-to-end acknowledgments for the clickhouse sink.

When requests encounter 500-level errors, the end status is now "Errored" instead of "Rejected".
