package metadata

components: sinks: pulsar: {
	title: "Apache Pulsar"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["text", "json"]
				}
			}
			request: enabled: false
			tls: enabled:     false
			to: {
				service: services.pulsar

				interface: {
					socket: {
						api: {
							title: "Pulsar protocol"
							url:   urls.pulsar_protocol
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "disabled"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		auth: {
			common:      false
			description: "Options for the authentication strategy."
			required:    false
			type: object: {
				examples: []
				options: {
					name: {
						common:      false
						description: "The basic authentication name."
						required:    false
						type: string: {
							default: null
							examples: ["${PULSAR_NAME}", "name123"]
						}
					}
					token: {
						common:      false
						description: "The basic authentication password."
						required:    false
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
			type: string: {
				examples: ["topic-1234"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		encode_errors_total: components.sources.internal_metrics.output.metrics.encode_errors_total
	}
}
