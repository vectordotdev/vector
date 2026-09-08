The `reduce` transform now supports a `max_groups` option that limits the number of groups kept in memory at any one time, as a safeguard against unbounded memory growth when `group_by` contains high-cardinality fields. When a new group would exceed the limit, the group that has gone the longest without receiving an event is flushed early, and the new `reduce_max_groups_exceeded_total` internal metric is incremented.

authors: karan-vk
