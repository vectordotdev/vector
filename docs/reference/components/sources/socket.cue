package metadata

components: sources: socket: {
	title:             "Socket"
	short_description: "Ingests data through a [socket][urls.socket], such as a [TCP][urls.tcp], [UDP][urls.udp], or [UDS][urls.uds] socket and outputs log events."
	long_description:  "Ingests data through a [socket][urls.socket], such as a [TCP][urls.tcp], [UDP][urls.udp], or [UDS][urls.uds] socket and outputs log events."

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator", "sidecar"]
		egress_method: "stream"
		function:      "receive"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			enabled_default:        false
		}
	}

	statuses: {
		delivery:    "best_effort"
		development: "stable"
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
		warnings: [
			"""
				This component exposes a configured port. You must ensure your network allows access to this port.
				""",
		]
		notices: []
	}

	configuration: {
		address: {
			description: "The address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port.\n"
			groups: ["tcp", "udp"]
			required: true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:9000", "systemd", "systemd#3"]
			}
		}
		host_key: {
			common:      false
			description: "The key name added to each event representing the current host. This can also be globally set via the [global `host_key` option][docs.reference.global-options#host_key]."
			groups: ["tcp", "udp", "unix"]
			required: false
			warnings: []
			type: string: {
				default: "host"
			}
		}
		max_length: {
			common:      true
			description: "The maximum bytes size of incoming messages before they are discarded."
			groups: ["tcp", "udp", "unix"]
			required: false
			warnings: []
			type: uint: {
				default: 102400
				unit:    "bytes"
			}
		}
		mode: {
			description: "The type of socket to use."
			groups: ["tcp", "udp", "unix"]
			required: true
			warnings: []
			type: string: {
				enum: {
					tcp:  "TCP Socket."
					udp:  "UDP Socket."
					unix: "Unix Domain Socket."
				}
			}
		}
		path: {
			description: "The unix socket path. *This should be absolute path*."
			groups: ["unix"]
			required: true
			warnings: []
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		shutdown_timeout_secs: {
			common:      false
			description: "The timeout before a connection is forcefully closed during shutdown."
			groups: ["tcp"]
			required: false
			warnings: []
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
	}

	output: logs: line: {
		description: "A single socket event."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
		}
	}

	examples: log: [
		{
			_line: #"""
				2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
				"""#
			title: "Socket line"
			configuration: {}
			input: """
				```text
				\( _line )
				```
				"""
			output: {
				timestamp: _values.current_timestamp
				message:   _line
				host:      _values.local_host
			}
		}]
}
