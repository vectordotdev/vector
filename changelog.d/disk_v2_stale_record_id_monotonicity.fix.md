The `disk` buffer no longer treats a record whose ID is behind the last acknowledged ID as a gap of nearly 2^64 events. Previously, when no records were pending acknowledgement, such a record was silently "skipped" (logged as `Events dropped. count=1844674407370946xxxx ... reason=unprocessable_events`), the reader's position was wrapped backwards, and the sink's buffer stalled without crashing until Vector was restarted. The condition is now reported as a record ID monotonicity violation, which surfaces the fault immediately instead of leaving the buffer wedged.

authors: giannimassi
