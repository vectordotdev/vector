Sinks using batch encoding (Parquet, Arrow IPC) now consistently emit `ComponentEventsDropped` for every encode failure path. Previously some `build_record_batch` failures (notably type mismatches) dropped events silently. A new `EncoderRecordBatchError` internal event also reports `component_errors_total` with `error_code="arrow_json_decode"` or `"arrow_record_batch_creation"` at `stage="sending"` for granular alerting.

authors: pront
