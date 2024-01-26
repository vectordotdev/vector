package metadata

components: _amqp: {
	features: {
		send: to: {
			service: services.amqp
			interface: {
				socket: {
					api: {
						title: "AMQP protocol"
						url:   urls.amqp_protocol
					}
					direction: "outgoing"
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			enabled_default:        false
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
		connection_string: {
			description: """
				Connection string to use when connecting to an AMQP server in the format of amqp://user:password@host:port/vhost?timeout=seconds.
				The default vhost can be represented as %2f.
				"""
			required: true
			warnings: []
			type: string: {
				examples: ["amqp://user:password@127.0.0.1:5672/%2f?timeout=10"]
				syntax: "literal"
			}
		}
	}

	how_it_works: {
		lapin: {
			title: "Lapin"
			body:  """
				The `amqp` source and sink uses [`lapin`](\(urls.lapin)) under the hood. This
				is a reliable pure rust library that facilitates communication with AMQP servers
				such as RabbitMQ.
				"""
		}
	}
}
