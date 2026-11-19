The `throttle` transform has a new `concurrency` option that sets how many tasks rate limit events in parallel. Events are assigned to tasks by hashing their rendered `key_field`, so each bucket is still rate limited by a single task, in arrival order. With more than one task, events of different buckets can come out in a different order. The default is `1`, which keeps the current behavior. The option has no effect without `key_field`.

authors: armleth
