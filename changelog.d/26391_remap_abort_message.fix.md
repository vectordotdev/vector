The `remap` transform's "Event mapping aborted." debug log now includes the message given to `abort` in its `error` field, so the reason for a dropped event is no longer lost when `reroute_dropped` is not enabled.

authors: anpawo
