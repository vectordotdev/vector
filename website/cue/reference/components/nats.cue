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
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		url: {
			description: "The NATS URL to connect to. The url _must_ take the form of `nats://server:port`."
			required:    true
			type: string: {
				examples: ["nats://demo.nats.io", "nats://127.0.0.1:4222"]
			}
		}
		subject: {
			description: "The NATS subject to publish messages to."
			required:    true
			type: string: {
				examples: ["{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
				syntax: "template"
			}
		}
		connection_name: {
			common:      false
			description: "A name assigned to the NATS connection."
			required:    false
			type: string: {
				default: "vector"
				examples: ["foo", "API Name Option Example"]
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
}
