package metadata

components: sinks: honeycomb: {
	title: "Honeycomb"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Honeycomb"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    5242880
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: false
			to: {
				service: services.honeycomb

				interface: {
					socket: {
						api: {
							title: "Honeycomb batch events API"
							url:   urls.honeycomb_batch
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
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
		api_key: {
			description: "The team key that will be used to authenticate against Honeycomb."
			required:    true
			warnings: []
			type: string: {
				examples: ["${HONEYCOMB_API_KEY}", "some-api-key"]
				syntax: "literal"
			}
		}
		dataset: {
			description: "The dataset that Vector will send logs to."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-honeycomb-dataset"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Register for a free account at [honeycomb.io](\(urls.honeycomb_signup))

				2. Once registered, create a new dataset and when presented with log shippers select the
				curl option and use the key provided with the curl example.
				"""
		}
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
