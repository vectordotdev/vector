package metadata

components: sinks: socket: {
	title: "Socket"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		development:   "stable"
		egress_method: "stream"
		service_providers: []
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: null
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: {
					name:     "socket receiver"
					thing:    "a \(name)"
					url:      urls.socket
					versions: null
				}

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp", "udp", "unix"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		targets: {
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
		address: {
			description: "The address to connect to. The address _must_ include a port."
			groups: ["tcp", "udp"]
			required: true
			warnings: []
			type: string: {
				examples: ["92.12.333.224:5000"]
			}
		}
		mode: {
			description: "The type of socket to use."
			groups: ["tcp", "udp", "unix"]
			required: true
			warnings: []
			type: string: {
				enum: {
					tcp:  "TCP socket"
					udp:  "UDP socket"
					unix: "Unix domain socket"
				}
			}
		}
		path: {
			description: "The unix socket path. This should be the absolute path."
			groups: ["unix"]
			required: true
			warnings: []
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
