package metadata

components: _opcua: {
	features: {
		collect: from: {
			service: services.opcua
			interface: {
				socket: {
					api: {
						title: "OPCUA protocol"
						url:   urls.opcua
					}
					direction: "incoming",
					protocols: ["tcp"],
					port: 4840,
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
		opcua_rs: {
			title: "opcua.rs"
			body:  """
				The `opcua` source uses [`opcua.rs`](\(urls.opcua_rs)) under the hood.
				"""
		}
	}
}
