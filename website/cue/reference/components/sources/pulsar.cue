package metadata

components: sources: pulsar: {
	title: "Apache Pulsar"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
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
					oauth2: {
						common:      false
						description: "Options for OAuth2 authentication."
						required:    false
						type: object: {
							examples: []
							options: {
								issuer_url: {
									description: "The issuer url."
									required:    true
									type: string: {
										examples: ["${OAUTH2_ISSUER_URL}", "https://oauth2.issuer"]
									}
								}
								credentials_url: {
									description: "The url for credentials. The data url is also supported."
									required:    true
									type: string: {
										examples: ["{OAUTH2_CREDENTIALS_URL}", "file:///oauth2_credentials", "data:application/json;base64,cHVsc2FyCg=="]
									}
								}
								audience: {
									common:      false
									description: "OAuth2 audience."
									required:    false
									type: string: {
										default: null
										examples: ["${OAUTH2_AUDIENCE}", "pulsar"]
									}
								}
								scope: {
									common:      false
									description: "OAuth2 scope."
									required:    false
									type: string: {
										default: null
										examples: ["${OAUTH2_SCOPE}", "admin"]
									}
								}
							}
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
		topics: {
			description: "The Pulsar topic names to read events from."
			required:    true
			type: string: {
				examples: ["topic-1234"]
			}
		}
	}

	output: logs: record: {
		description: "An individual Pulsar record"
		fields: {
			message: {
				description: "The raw line from the Kafka record."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["pulsar"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The current time if it cannot be fetched."
			}
			publish_time: fields._current_timestamp & {
				description: "The timestamp encoded in the Pulsar message."
			}
			topic: {
				description: "The Pulsar topic that the record came from."
				required:    true
				type: string: {
					examples: ["topic"]
				}
			}
			producer_name: {
				description: "The Pulsar producer's name which the record came from."
				required:    true
				type: string: {
					examples: ["pulsar-client"]
				}
			}
		}
	}

	telemetry: metrics: {
		component_discarded_events_total: components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:           components.sources.internal_metrics.output.metrics.component_errors_total
	}
}
