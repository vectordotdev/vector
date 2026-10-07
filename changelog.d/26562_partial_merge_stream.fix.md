The `kubernetes_logs` source no longer merges `stdout` and `stderr` together when reassembling partial (CRI `P`) log lines. Because CRI interleaves both streams in the same container log file, a complete line on one stream that arrived between a partial line and its continuation on the other stream was previously appended to the buffered partial event, producing a corrupted cross-stream record. Partial fragments are now grouped by both file and stream, so only fragments from the same stream are merged.

authors: jcantrill
