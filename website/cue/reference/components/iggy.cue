package metadata

components: _iggy: {
	features: {
		collect: from: {
			service: services.iggy
			interface: {
				socket: {
					api: {
						title: "Iggy protocol"
						url:   urls.iggy
					}
					direction: "incoming"
					port:      8090
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}

		send: to: {
			service: services.iggy
			interface: {
				socket: {
					api: {
						title: "Iggy protocol"
						url:   urls.iggy
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
		connection_string: {
			title: "Connection string"
			body: """
				The `iggy` source/sink connects to a broker through Iggy's
				[connection string][iggy_connection_string] format
				(`iggy+<protocol>://<credentials>@<host>:<port>`). The protocol
				selects one of `tcp`, `quic`, `http`, or `ws`, and credentials
				are either `username:password` or a personal access token. The
				legacy `iggy://user:pass@host:port` form is also accepted and
				resolves to TCP.
				"""
		}
	}
}
