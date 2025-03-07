The `kubernetes_logs` source now supports a new configuration called `max_merged_line_bytes` which allows limiting the size
of lines even when they have been assembled via `auto_partial_merge` (the existing `max_line_bytes` field only applies
before merging, and as such makes it impossible to limit lines assembled via merging, short of specifying a size so small
that the continuation character isn't reached, and merging doesn't happen at all).

authors: ganelo
