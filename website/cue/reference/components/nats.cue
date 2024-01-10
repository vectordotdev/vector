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

	how_it_works: {
		nats_rs: {
			title: "nats.rs"
			body:  """
				The `nats` source/sink uses [`nats.rs`](\(urls.nats_rs)) under the hood.
				"""
		}
	}
}
