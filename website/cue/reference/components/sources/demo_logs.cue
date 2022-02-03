package metadata

components: sources: demo_logs: {
	title: "Demo Logs"

	alias: "generator"

	description: """
		Generates fakes events useful for testing and demoing.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
		generate: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		format: {
			description: "The format of the randomly generated output."
			required:    true
			type: string: {
				enum: {
					"shuffle":       "Lines are chosen at random from the list specified using `lines`."
					"apache_common": "Randomly generated logs in [Apache common](\(urls.apache_common)) format."
					"apache_error":  "Randomly generated logs in [Apache error](\(urls.apache_error)) format."
					"syslog":        "Randomly generated logs in Syslog format ([RFC 5424](\(urls.syslog_5424)))."
					"bsd_syslog":    "Randomly generated logs in Syslog format ([RFC 3164](\(urls.syslog_3164)))."
					"json":          "Randomly generated HTTP server logs in [JSON](\(urls.json)) format."
				}
			}
		}
		interval: {
			common: false
			description: """
				The amount of time, in seconds, to pause between each batch of output lines. The
				default is one batch per second. In order to remove the delay and output batches as
				quickly as possible, set `interval` to `0.0`.
				"""
			required: false
			type: float: {
				default: 1.0
				examples: [1.0, 0.1, 0.01]
			}
		}
		count: {
			common:      false
			description: "The total number of lines to output. By default the source continuously prints logs (infinitely)."
			required:    false
			type: uint: {
				default: null
				unit:    null
			}
		}
		lines: {
			common:        false
			description:   "The list of lines to output."
			relevant_when: "`format` = `shuffle`"
			required:      false
			type: array: {
				default: null
				items: type: string: {
					examples: ["Line 1", "Line 2"]
				}
			}
		}
		sequence: {
			common:        false
			relevant_when: "`format` = `shuffle`"
			description:   "If `true`, each output line starts with an increasing sequence number, beginning with 0."
			required:      false
			type: bool: default: false
		}
	}

	output: logs: {}

	telemetry: metrics: {
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
