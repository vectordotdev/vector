The `reduce` transform now reduces groups in parallel, across the number of tasks set by the new `concurrency` option (default `8`). All events of a group are still reduced by the same task, in arrival order, so each group is reduced exactly as before. Only the order of output events across different groups may change. The option has no effect when `group_by` is empty; set it to `1` to reduce in a single task.

authors: armleth
