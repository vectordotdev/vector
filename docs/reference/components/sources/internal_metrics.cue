package metadata

components: sources: internal_metrics: {
	title:       "Internal Metrics"
	description: "The internal metrics source exposes metrics emitted by the running Vector instance (as opposed to components in its topology)."

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				name:     "Vector instance"
				thing:    "a \(name)"
				url:      urls.vector_docs
				versions: ">= 0.11.0"
			}
		}
		multiline: enabled: false
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		notices: []
		requirements: []
		warnings: []
	}

	output: metrics: {
		// Default internal metrics tags
		_internal_metrics_tags: {
			instance: {
				description: "The Vector instance identified by host and port."
				required:    true
				examples: [_values.instance]
			}
			job: {
				description: "The name of the job producing Vector metrics."
				required:    true
				default:     "vector"
			}
		}

		api_started_total: {
			description:       "The number of times the Vector GraphQL API has been started."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		config_load_errors_total: {
			description:       "The total number of errors loading the Vector configuration."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		connection_errors_total: {
			description:       "The total number of connection errors for this Vector instance."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		quit_total: {
			description:       "The total number of times the Vector instance has quit."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		recover_errors_total: {
			description:       "The total number of errors caused by Vector failing to recover from a failed reload."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		reload_errors_total: {
			description:       "The total number of errors encountered when reloading Vector."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		reloaded_total: {
			description:       "The total number of times the Vector instance has been reloaded."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		started_total: {
			description:       "The total number of times the Vector instance has been started."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		stopped_total: {
			description:       "The total number of times the Vector instance has been stopped."
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
	}
}
