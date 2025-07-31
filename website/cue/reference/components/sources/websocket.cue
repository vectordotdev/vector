package metadata

components: sources: websocket: {
	title: "WebSocket"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: false
		auto_generated:   true
		codecs: {
			enabled:         true
			default_framing: "message_based"
		}
		multiline: enabled: false
		collect: {
			checkpoint: {
				enabled: false
			}
			proxy: {
				enabled: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_by_scheme:      true
				enabled_default:        false
			}
			from: {
				service: services.websocket
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
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
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.sources.websocket.configuration & {
		ping_timeout: warnings: ["This option is ignored if the `ping_interval` option is not set."]
	}

	configuration_examples: [
		{
			title: "Common"
			configuration: {
				type: "websocket"
				uri:  "ws://127.0.0.1:8080/events"
			}
		},
		{
			title: "Advanced"
			configuration: {
				type:            "websocket"
				uri:             "wss://data.example.com/stream"
				initial_message: "SUBSCRIBE logs"
				auth: {
					strategy: "basic"
					user:     "my_user"
					password: "${WS_PASSWORD}"
				}
				tls: {
					enabled: true
				}
				ping_interval: 30
				ping_timeout:  10
				ping_message:  "PING"
				pong_message:  "PONG"
			}
		},
	]

	output: {
		logs: event: {
			description: "An event received from the WebSocket server."
			fields: {
				message: {
					description: "The raw message payload."
					required:    true
					type: string: {
						examples: ["{\"level\":\"info\",\"message\":\"foo\"}"]
					}
				}
				source_type: {
					description: "The component type."
					required:    true
					type: string: {
						examples: ["websocket"]
					}
				}
				timestamp: fields._current_timestamp
			}
		}
	}
}
