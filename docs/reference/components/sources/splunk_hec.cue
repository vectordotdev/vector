package metadata

components: sources: splunk_hec: {
	title:             "Splunk HEC"
	short_description: "Ingests data through the [Splunk HTTP Event Collector protocol][urls.splunk_hec_protocol] and outputs log events."
	long_description:  "The [Splunk HTTP Event Collector (HEC)][urls.splunk_hec] is a fast and efficient way to send data to Splunk Enterprise and Splunk Cloud. Notably, HEC enables you to send data over HTTP (or HTTPS) directly to Splunk Enterprise or Splunk Cloud from your application."

	classes: {
		commonly_used: false
		deployment_roles: ["aggregator"]
		egress_method: "batch"
		function:      "receive"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			enabled_default:        false
		}
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
	}

	support: {
		platforms: {
			triples: {
				"aarch64-unknown-linux-gnu":  true
				"aarch64-unknown-linux-musl": true
				"x86_64-apple-darwin":        true
				"x86_64-pc-windows-msv":      true
				"x86_64-unknown-linux-gnu":   true
				"x86_64-unknown-linux-musl":  true
			}
		}

		requirements: [
			"""
				This component exposes a configured port. You must ensure your network allows access to this port.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		address: {
			common:      true
			description: "The address to accept connections on."
			required:    false
			warnings: []
			type: string: {
				default: "0.0.0.0:8088"
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
}
