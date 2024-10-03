package metadata

components: sources: static_metrics: {
	title: "Static Metrics"

	description: """
		Publish statically configured metrics on an interval. This can be useful for publishing
		heartbeats or sending the value of an environment variable as a metric.
		"""

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "daemon", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: service:       services.vector
		}
		multiline: enabled: false
	}

	support: {
		notices: []
		requirements: []
		warnings: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.static_metrics.configuration

	output: metrics: {
		counter: output._passthrough_counter & {
			default_namespace: "static"
		}
		distribution: output._passthrough_distribution & {
			default_namespace: "static"
		}
		gauge: output._passthrough_gauge & {
			default_namespace: "static"
		}
		set: output._passthrough_set & {
			default_namespace: "static"
		}
	}

	examples: [
		{
			title: "Emit a heartbeat"
			configuration: {
				metrics: [
					{
						name: "heartbeat"
						kind: "absolute"
						value:
							gauge: 1
						tags:
							env: "${ENV}"
					},
				]
			}
			input: ""
			output: metric: {
				name:      "heartbeat"
				kind:      "absolute"
				namespace: "static"
				timestamp: "2024-09-10T19:04:58Z"
				gauge:
					value: 1.0
				tags:
					env: "${ENV}"
			}
		},
	]
}
