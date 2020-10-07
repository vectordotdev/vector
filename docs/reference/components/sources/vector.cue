package metadata

components: sources: vector: {
	title: "Vector"
	short_description: "Ingests data through another upstream [`vector` sink][docs.sinks.vector] and outputs log and metric events."
	long_description: "Ingests data through another upstream [`vector` sink][docs.sinks.vector] and outputs log and metric events."

	classes: {
		commonly_used: false
		deployment_roles: ["service"]
		function: "receive"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled: false
		tls: {
			enabled: true
			can_enable: true
			can_verify_certificate: true
			can_verify_hostname: false
			enabled_default: false
		}
	}

	statuses: {
		delivery: "best_effort"
		development: "beta"
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu": true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin": true
			"x86_64-pc-windows-msv": true
			"x86_64-unknown-linux-gnu": true
			"x86_64-unknown-linux-musl": true
		}

		requirements: [
			"""
			This component exposes a configured port. You must ensure your network allows access to this port.
			""",
		]
		warnings: []
	}

	configuration: {
		address: {
			description: "The TCP address to listen for connections on, or `systemd#N to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port.\n"
			required: true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:9000","systemd","systemd#1"]
			}
		}
		shutdown_timeout_secs: {
			common: false
			description: "The timeout before a connection is forcefully closed during shutdown."
			required: false
			warnings: []
			type: uint: {
				default: 30
				unit: "seconds"
			}
		}
	}

	how_it_works: {
		message_acking: {
			title: "Message Ack'ing"
			body: "Currently, Vector does not perform any application level message acknowledgement. While rare, this means the individual message could be lost."
		}

		tcp_protocol: {
			title: "TCP Protocol"
			body: "Upstream Vector instances forward data to downstream Vector instances via the TCP protocol."
		}
	}
}
