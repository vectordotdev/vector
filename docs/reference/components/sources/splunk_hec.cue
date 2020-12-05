package metadata

components: sources: splunk_hec: {
	_port: 8080

	title:       "Splunk HTTP Event Collector (HEC)"
	description: "The [Splunk HTTP Event Collector (HEC)](\(urls.splunk_hec)) is a fast and efficient way to send data to Splunk Enterprise and Splunk Cloud. Notably, HEC enables you to send data over HTTP (or HTTPS) directly to Splunk Enterprise or Splunk Cloud from your application."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "batch"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.splunk

				interface: socket: {
					api: {
						title: "Splunk HEC"
						url:   urls.splunk_hec_protocol
					}
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "optional"
				}
			}

			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		address: {
			common:      true
			description: "The address to accept connections on."
			required:    false
			warnings: []
			type: string: {
				default: "0.0.0.0:\(_port)"
			}
		}
		token: {
			common:      true
			description: "If supplied, incoming requests must supply this token in the `Authorization` header, just as a client would if it was communicating with the Splunk HEC endpoint directly. If _not_ supplied, the `Authorization` header will be ignored and requests will not be authenticated."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["A94A8FE5CCB19BA61C4C08"]
			}
		}
	}

	output: logs: event: {
		description: "A single event"
		fields: {
			message: fields._raw_line
			splunk_channel: {
				description: "The Splunk channel, value of the `X-Splunk-Request-Channel` header."
				required:    true
				type: timestamp: {}
			}
			timestamp: fields._current_timestamp
		}
	}

	telemetry: metrics: {
		http_request_errors_total: components.sources.internal_metrics.output.metrics.http_request_errors_total
		requests_received_total:   components.sources.internal_metrics.output.metrics.requests_received_total
	}
}
