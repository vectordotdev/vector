Added the `files_unwatched_bytes_unread_total` internal metric to the `file` and `kubernetes_logs` sources. It tracks known unread bytes when a file is unwatched, for example after deletion or rotation. A known count of zero means no bytes remained unread at measurement time.

When the unread-byte count is unavailable, Vector increments `files_unwatched_with_unknown_bytes_total` instead. This counter counts unwatch events with unknown unread size, including gzipped files, skipped gzip readers, and metadata failures; it does not measure lost bytes. Gzip files are not decompressed solely to calculate telemetry. Both metrics use the existing `reached_eof` label and optional `file` label, and `files_unwatched_total` continues to count all unwatch events.

authors: akashvbabu91
