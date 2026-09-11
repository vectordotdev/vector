The `file` source now supports an opt-in `file_discovery_mode: notify` setting that uses OS-level
file system event notifications (inotify on Linux, FSEvents on macOS, `ReadDirectoryChangesW` on
Windows) instead of periodic glob re-scanning to discover new files and wake up reads. This avoids
the cost of re-globbing and re-fingerprinting every matched file on a fixed interval. A much less
frequent periodic reconciliation pass (`reconcile_interval_secs`) still runs as a correctness
backstop. The default remains the existing polling-based `file_discovery_mode: polling` behavior.

Separately (and independently of `file_discovery_mode`), a new `idle_timeout_secs` option (default:
60 seconds) closes a file's handle once it has reached EOF and received no new data for that long,
avoiding holding a large number of open file handles for files that are being watched but aren't
actively being written to. See the `idle_timeout_secs` documentation for details.

authors: sashamelentiev
