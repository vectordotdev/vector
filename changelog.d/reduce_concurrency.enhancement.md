The `reduce` transform has a new `concurrency` option that sets how many tasks reduce events in parallel. Events are assigned to tasks by hashing their `group_by` values, so each group is still merged in arrival order. With more than one task, finished groups can come out in a different order. The default is `1`, which keeps the current behavior.

authors: armleth
