Fixed the internal buffer usage reporter outliving the buffer it reports on,
which would cause the reporter for the old buffer to keep publishing stale
metrics under the same `buffer_id` as its replacement.  This also lets the
metrics for a buffer that was removed rather than replaced age out under
`expire_metrics_secs`, which the continuously republished values previously
prevented.

authors: bruceg
