The `sample` transform can now take in a `group_by` configuration option that will allow logs with unique values for the patterns passed in to be sampled independently. This can reduce the complexity of the topology, since users would no longer need to create separate samplers with similar configuration to handle different log streams.

authors: hillmandj
