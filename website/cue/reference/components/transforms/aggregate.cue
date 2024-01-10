package metadata

components: transforms: aggregate: {
	title: "Aggregate"

	description: """
		Aggregates multiple metric events into a single metric event based
		on a defined interval window. This helps to reduce metric volume at
		the cost of granularity.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		aggregate: {}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: base.components.transforms.aggregate.configuration

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

	examples: [
		{
			title: "Aggregate over 5 seconds"
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
							value: 2.2
						}
					}
				},
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "different.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "gauge.1"
						timestamp: "2021-07-12T07:58:47.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 22.33
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "gauge.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 44.55
						}
					}
				},
			]
			configuration: {
				interval_ms: 5000
			}
			output: [
				{
					metric: {
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
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
						kind:      "incremental"
						name:      "counter.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "different.host.com"
						}
						counter: {
							value: 1.1
						}
					}
				},
				{
					metric: {
						kind:      "absolute"
						name:      "gauge.1"
						timestamp: "2021-07-12T07:58:45.223543Z"
						tags: {
							host: "my.host.com"
						}
						counter: {
							value: 44.55
						}
					}
				},
			]
		},
	]

	how_it_works: {
		aggregation_behavior: {
			title: "Aggregation Behavior"
			body: """
				Metrics are aggregated based on their kind. During an interval, `incremental` metrics
				are "added" and newer `absolute` metrics replace older ones in the same series. This results in a reduction
				of volume and less granularity, while maintaining numerical correctness. As an example, two
				`incremental` `counter` metrics with values 10 and 13 processed by the transform during a period would be
				aggregated into a single `incremental` `counter` with a value of 23. Two `absolute` `gauge` metrics with
				values 93 and 95 would result in a single `absolute` `gauge` with the value of 95. More complex
				types like `distribution`, `histogram`, `set`, and `summary` behave similarly with `incremental`
				values being combined in a manner that makes sense based on their type.
				"""
		}

		advantages: {
			title: "Advantages of Use"
			body: """
				The major advantage to aggregation is the reduction of volume. It may reduce costs
				directly in situations that charge by metric event volume, or indirectly by requiring less CPU to
				process and/or less network bandwidth to transmit and receive. In systems that are constrained by
				the processing required to ingest metric events it may help to reduce the processing overhead. This
				may apply to transforms and sinks downstream of the aggregate transform as well.
				"""
		}

	}

	telemetry: metrics: {
		aggregate_events_recorded_total: components.sources.internal_metrics.output.metrics.aggregate_events_recorded_total
		aggregate_failed_updates:        components.sources.internal_metrics.output.metrics.aggregate_failed_updates
		aggregate_flushes_total:         components.sources.internal_metrics.output.metrics.aggregate_flushes_total
	}
}
