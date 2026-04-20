Sources now record the distribution metrics `source_send_latency_seconds` (measuring the time spent
blocking on a single events chunk send operation on the output) and `source_send_batch_latency_seconds`
(encompassing all chunks within a received events batch).

authors: gwenaskell
