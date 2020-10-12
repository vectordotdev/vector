package metadata

components: sinks: honeycomb: {
	title:             "Honeycomb"
	short_description: "Batches log events to [Honeycomb][urls.honeycomb] via the [batch events API][urls.honeycomb_batch]."
	long_description:  "[Honeycomb][urls.honeycomb] provides full stack observability—designed for high cardinality data and collaborative problem solving, enabling engineers to deeply understand and debug production software together."

	classes: {
		commonly_used: false
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["Honeycomb"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    5242880
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
		api_key: {
			description: "The team key that will be used to authenticate against Honeycomb."
			required:    true
			warnings: []
			type: string: {
				examples: ["${HONEYCOMB_API_KEY}", "some-api-key"]
			}
		}
		dataset: {
			description: "The dataset that Vector will send logs to."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-honeycomb-dataset"]
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
				1. Register for a free account at [honeycomb.io][urls.honeycomb_signup]

				2. Once registered, create a new dataset and when presented with log shippers select the
				curl option and use the key provided with the curl example.
				"""#
		}
	}
}
