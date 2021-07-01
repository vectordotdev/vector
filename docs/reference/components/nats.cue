package metadata

components: _nats: {
	features: {
		collect: from: {
			service: services.nats
			interface: {
				socket: {
					api: {
						title: "NATS protocol"
						url:   urls.nats
					}
					direction: "incoming"
					port:      4222
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}

		send: to: {
			service: services.nats
			interface: {
				socket: {
					api: {
						title: "NATS protocol"
						url:   urls.nats
					}
					direction: "outgoing"
					protocols: ["tcp"]
					ssl: "optional"
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
		url: {
			description: "The NATS URL to connect to. The url _must_ take the form of `nats://server:port`."
			required:    true
			warnings: []
			type: string: {
				examples: ["nats://demo.nats.io", "nats://127.0.0.1:4222"]
				syntax: "literal"
			}
		}
		subject: {
			description: "The NATS subject to publish messages to."
			required:    true
			warnings: []
			type: string: {
				examples: ["{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
				syntax: "template"
			}
		}
		name: {
			common:      false
			description: "A name assigned to the NATS connection."
			required:    false
			type: string: {
				default: "vector"
				examples: ["foo", "API Name Option Example"]
				syntax: "literal"
			}
		}
	}

	how_it_works: {
		nats_rs: {
			title: "nats.rs"
			body:  """
				The `nats` source/sink uses [`nats.rs`](\(urls.nats_rs)) under the hood.
				"""
		}
	}

	telemetry: metrics: {
		events_discarded_total:  components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:  components.sources.internal_metrics.output.metrics.processed_events_total
		send_errors_total:       components.sources.internal_metrics.output.metrics.send_errors_total
	}
}
