Previously, when the `auto_extract_timestamp` setting in the `splunk_hec_logs` Sink was enabled, the sink was attempting to remove the existing event timestamp. This would throw a warning that the timestamp type was invalid.

This has been fixed to correctly not attempt to remove the timestamp from the event if `auto_extract_timestamp` is enabled, since this setting indicates that Vector should let Splunk do that.
