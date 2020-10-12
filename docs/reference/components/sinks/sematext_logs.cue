package metadata

components: sinks: sematext_logs: {
	title:             "Sematext Logs"
	short_description: "Batches log events to [Sematext][urls.sematext] via the [Elasticsearch API][urls.sematext_es]."
	long_description:  "[Sematext][urls.sematext] is a hosted monitoring platform based on Elasticsearch. Providing powerful monitoring and management solutions to monitor and observe your apps in real-time."

	classes: {
		commonly_used: false
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["Sematext"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    10490000
			max_events:   null
			timeout_secs: 1
		}
		buffer: enabled:      true
		compression: enabled: false
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: {
			enabled:                    true
			in_flight_limit:            5
			rate_limit_duration_secs:   1
			rate_limit_num:             5
			retry_initial_backoff_secs: 1
			retry_max_duration_secs:    10
			timeout_secs:               60
		}
		tls: enabled: false
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

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		endpoint: {
			common:      false
			description: "The endpoint that will be used to send logs to. This option is required if `region` is not set."
			required:    false
			type: string: {
				default: null
				examples: ["http://127.0.0.1", "http://example.com"]
			}
		}
		region: {
			description:   "The region destination to send metrics to. This option is required if `endpoint` is not set."
			required:      true
			relevant_when: "`endpoint` is not set"
			warnings: []
			type: string: {
				enum: {
					us: "United States"
					eu: "Europe"
				}
				examples: [ "us"]
			}
		}
		token: {
			description: "The token that will be used to write to Sematext."
			required:    true
			warnings: []
			type: string: {
				examples: ["${SEMATEXT_TOKEN}", "some-sematext-token"]
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
				1. Register for a free account at [Sematext.com][urls.sematext_registration]

				2. [Create a Logs App][urls.sematext_create_logs_app] to get a Logs Token
				for [Sematext Logs][urls.sematext_logsense]
				"""#
		}
	}
}
