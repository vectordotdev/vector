package metadata

components: sources: datadog_logs: {
	_port: 8080

	title: "Datadog Logs"

	description: """
		Receives logs from a Datadog Agent over HTTP or HTTPS.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.datadog_logs

				interface: socket: {
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

	installation: {
		platform_name: null
	}

	configuration: {
		address: sources.http.configuration.address
		drop_invalid_api_key: {
			description: """
				The flag to indicate whether to drop the event with an invalid dd_api_key.
				If the event has no dd_api_key set, then the event would never be dropped.
				"""
			required: true
			type: bool: {
				default: false
			}
		}
	}

	output: logs: line: {
		description: "An individual event from a batch of events received through an HTTP POST request sent by a Datadog Agent."
		fields: {
			message: {
				description: "The message field, containing the plain text message."
				required:    true
				type: string: {
					examples: ["Hi from erlang"]
					syntax: "literal"
				}
			}
			status: {
				description: "The status field extracted from the event."
				required:    true
				type: string: {
					examples: ["info"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp
			hostname:  fields._local_host
			service: {
				description: "The service field extracted from the event."
				required:    true
				type: string: {
					examples: ["backend"]
					syntax: "literal"
				}
			}

			dd_api_key: {
				description: """
					The Datadog API key extracted from the event. This sensitive field may be removed
					or obfuscated using the `remap` transform.
					"""
				required: true
				type: string: {
					examples: ["abcdefgh13245678abcdefgh13245678"]
					syntax: "literal"
				}
			}
			ddsource: {
				description: "The source field extracted from the event."
				required:    true
				type: string: {
					examples: ["java"]
					syntax: "literal"
				}
			}
			ddtags: {
				description: "The coma separated tags list extracted from the event."
				required:    true
				type: string: {
					examples: ["env:prod,region:ap-east-1"]
					syntax: "literal"
				}
			}
		}
	}
}
