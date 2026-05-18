package metadata

components: sources: internal_metrics: {
	title: "Internal Metrics"

	description: """
		Exposes Vector's own internal metrics, allowing you to collect, process,
		and route Vector's internal metrics just like other metrics.
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

	configuration: generated.components.sources.internal_metrics.configuration

	how_it_works: {
		unique_series: {
			title: "Sending metrics from multiple Vector instances"
			body: """
				When sending `internal_metrics` from multiple Vector instances
				to the same destination, you will typically want to tag the
				metrics with a tag that is unique to the Vector instance sending
				the metrics to avoid the metric series conflicting. The
				`tags.host_key` option can be used for this, but you can also
				use a subsequent `remap` transform to add a different unique
				tag from the environment.
				"""
		}
	}
}
