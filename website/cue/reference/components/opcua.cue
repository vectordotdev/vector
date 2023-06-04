package metadata

components: _opcua: {
	features: {
		collect: from: {
			service: services.nats
			interface: {
				socket: {
					api: {
						title: "OPCUA protocol"
						url:   urls.opcua
					}
					direction: "incoming",
					url: "opc.tcp://localhost:4840"
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
			title: "opcua.rs"
			body:  """
				The `opcua` source uses [`opcua.rs`](\(urls.opcua_rs)) under the hood.
				"""
		}
	}
}
