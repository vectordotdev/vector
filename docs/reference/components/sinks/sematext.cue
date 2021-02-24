package metadata

components: sinks: _sematext: {
	features: {
		send: {
			to: {
				service: services.sematext

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}
	configuration: {
		endpoint: {
			common:        false
			description:   "The endpoint to send data to."
			relevant_when: "region is not set"
			required:      false
			type: string: {
				default: null
				examples: ["http://127.0.0.1", "http://example.com"]
				syntax: "literal"
			}
		}
		region: {
			description:   "The region to send data to."
			required:      true
			relevant_when: "endpoint is not set"
			warnings: []
			type: string: {
				enum: {
					us: "United States"
					eu: "Europe"
				}
				syntax: "literal"
			}
		}
		token: {
			description: "The token that will be used to write to Sematext."
			required:    true
			warnings: []
			type: string: {
				examples: ["${SEMATEXT_TOKEN}", "some-sematext-token"]
				syntax: "literal"
			}
		}
	}
}
