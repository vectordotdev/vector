package metadata

components: transforms: aggregate: {
	title: "Aggregate"

	description: """
		Aggregates multiple metric events into a single metric event based on a
		the MetricKind. Incremental metrics are "added", Absolute uses last
		value wins semantics.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		aggregate: {}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		interval_ms: {
			common: true
			description: """
				The interval over which metrics are aggregated in milliseconds. Over this period metrics with the
				same series data (name, namespace, tags, ...) will be aggregated.
				"""
			required: false
			warnings: []
			type: uint: {
				default: 10000
				unit:    "milliseconds"
			}
		}
	}

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
	}

	examples: [
		{
			title: "Aggregate over 15 seconds"
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
						name:      "guage.1"
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
						name:      "guage.1"
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
						name:      "guage.1"
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

	telemetry: metrics: {
		events_recorded_total: components.sources.internal_metrics.output.metrics.events_recorded_total
		flushed_total:         components.sources.internal_metrics.output.metrics.flushed_total
	}
}
