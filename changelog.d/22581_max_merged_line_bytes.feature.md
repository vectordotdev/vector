The `kubernetes_logs` source now includes a new `max_merged_line_bytes` configuration option. This setting enables users to cap the size of log lines after they’ve been combined using `auto_partial_merge`. Previously, the `max_line_bytes` field only restricted line sizes *before* merging, leaving no practical way to limit the length of merged lines—unless you set a size so tiny that it prevented merging altogether by stopping short of the continuation character. This new option gives you better control over merged line sizes.

authors: ganelo
