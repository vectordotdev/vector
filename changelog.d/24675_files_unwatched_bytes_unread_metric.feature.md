Added the `files_unwatched_bytes_unread_total` internal metric to the `file` and `kubernetes_logs` sources. This metric tracks the number of bytes that were not read from files before they were unwatched (e.g., due to file deletion or rotation), helping users identify potential data loss scenarios.

authors: akashvbabu91
