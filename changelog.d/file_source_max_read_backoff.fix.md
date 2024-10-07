The `file` and `kubernetes_logs` sources now check file modification times during file discovery runs and immediately read modified files, fixing an issue where infrequently modified files could take 10+ seconds to be read. This improvement is most effective with the `file` source and its default `glob_minimum_cooldown_ms` value of 1000 ms; values of 10000 ms (10 seconds) or more provide no benefit.

The previously hardcoded maximum backoff interval for reading inactive files is now configurable via `max_read_backoff_ms` (default: 2048 ms).

authors: htrendev
