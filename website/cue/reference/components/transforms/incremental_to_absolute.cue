package metadata

components: transforms: incremental_to_absolute: {
	title: "Incremental To Absolute"

	description: """
		Converts incremental metrics to absolute. Absolute metrics are emitted unchanged to downstream components.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		convert: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.incremental_to_absolute.configuration

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	output: {
		metrics: "": {
			description: "The modified input `metric` event."
		}
	}

	examples: [
		{
			title: "Convert incremental metrics to absolute"
			input: [
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:44.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:46.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T08:59:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
			]
			configuration: {
				cache: time_to_live: 10
			}
			output: [
				{
					metric: {
						kind:      "absolute"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:44.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 2.2
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:46.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 3.3
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "counter.1"
						timestamp: "2021-07-12T08:59:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
			]
		},
	]

	how_it_works: {
		advantages: {
			title: "Advantages of Use"
			body: """
				Converting incremental metrics to absolute metrics has two major benefits. First, incremental metrics require
				the entire history to determine the current state, as they depend on previous values to calculate changes.
				Each absolute metric represents a complete state, making it easier to view historical data accurately for
				components like the File sink, where some files might end up missing or out of order. Second, it can reduce
				overhead for downstream components like Prometheus Remote Write, which internally converts
				incremental to absolute metrics. Converting to absolute metric with this transform prevents the
				creation of duplicate caches when sending to multiple Prometheus Remote Write sinks.

				The conversion is performed based on the order in which incremental metrics are received, not their timestamps.
				Moreover, absolute metrics received by this transform are emitted unchanged.
				"""
		}
	}
}
