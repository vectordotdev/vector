Fixed an incorrect source_lag_time_seconds measurement in sources that use `send_batch` with large event batches. When a batch was split into multiple chunks, the reference timestamp used to compute lag time was re-captured on each chunk send, causing the lag time for later chunks to be overstated by the amount of time spent waiting for the channel to accept earlier chunks. The reference timestamp is now captured once before iteration and shared across all chunks.

authors: gwenaskell
