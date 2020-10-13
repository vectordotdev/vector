package metadata

components: sinks: pulsar: {
	title:             "Apache Pulsar"
	short_description: "Streams log events to [Apache Pulsar][urls.pulsar] via the [Pulsar protocol][urls.pulsar_protocol]."
	long_description:  "[Pulsar][urls.pulsar] is a multi-tenant, high-performance solution for server-to-server messaging. Pulsar was originally developed by Yahoo, it is under the stewardship of the Apache Software Foundation. It is an excellent tool for streaming logs and metrics data."

	classes: {
		commonly_used: false
		egress_method: "stream"
		function:      "transmit"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		compression: enabled: false
		encoding: codec: {
			enabled: true
			default: "text"
			enum: ["text", "json"]
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: enabled:         false
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
		auth: {
			common:      false
			description: "Options for the authentication strategy."
			required:    false
			warnings: []
			type: object: {
				examples: []
				options: {
					name: {
						common:      false
						description: "The basic authentication name."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["${PULSAR_NAME}", "name123"]
						}
					}
					token: {
						common:      false
						description: "The basic authentication password."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["${PULSAR_TOKEN}", "123456789"]
						}
					}
				}
			}
		}
		endpoint: {
			description: "Endpoint to which the pulsar client should connect to."
			required:    true
			type: string: {
				examples: ["pulsar://127.0.0.1:6650"]
			}
		}
		topic: {
			description: "The Pulsar topic name to write events to."
			required:    true
			warnings: []
			type: string: {
				examples: ["topic-1234"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}
}
