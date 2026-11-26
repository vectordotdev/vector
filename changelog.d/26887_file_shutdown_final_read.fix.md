Fixed the `file` and `kubernetes_logs` sources skipping lines appended during idle backoff just before shutdown. Both sources now perform one final read pass over watched files, subject to the existing per-file read limit, before closing their output.

authors: titaneric
