package metadata

components: sinks: papertrail: {
	title:             "Papertrail"
	short_description: "Streams log events to [Papertrail][urls.papertrail] via [Syslog][urls.papertrail_syslog]."
	long_description:  "[Papertrail][urls.papertrail] is a web-based log aggregation application used by developers and IT team to search and view logs in real time."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		function:      "transmit"
		service_providers: ["Papertrail"]
	}

	features: {
		buffer: enabled:      true
		compression: enabled: false
		encoding: codec: {
			enabled: true
			default: null
			enum: ["json", "text"]
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			can_verify_hostname:    true
			enabled_default:        true
		}
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

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		endpoint: {
			description: "The endpoint to send logs to."
			required:    true
			type: string: {
				examples: ["logs.papertrailapp.com:12345"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body: #"""
				1. Register for a free account at [Papertrailapp.com](https://papertrailapp.com/signup?plan=free)

				2. [Create a Log Destination](https://papertrailapp.com/destinations/new) to get a Log Destination
				and ensure that TCP is enabled.

				3. Set the log destination as the `endpoint` option and start shipping your logs!
				"""#
		}
	}
}
