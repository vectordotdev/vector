Fix disk-buffered records remaining undelivered when input becomes quiet because
pending file writes were not completed before the buffer reader was notified.
This applies to all sinks using disk buffers and requires no configuration change.

authors: mzd00
