package metadata

components: sinks: _cnosdb: {
	features: {
		send: {
			proxy: enabled: true
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.cnosdb

				interface: {
					socket: {
						api: {
							title: "CnosDB HTTP API"
							url:   urls.cnosdb_http_api
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	configuration: {
		tenant: {
			description: "The name of the tenant to write into. Default to 'cnosdb'."
			required: false
			type: string: {
      	default: "cnosdb"
				examples: ["cnosdb", "org1"]
			}
		}
		database: {
			description: "The name of the database to write into. Default to 'public'."
			required: false
			type: string: {
      	default: "public"
				examples: ["vector_logs", "public"]
			}
		}
		endpoint: {
			description: "The endpoint to send data to."
			required: true
			type: string: {
				examples: ["http://localhost:8902/"]
			}
		}
		user: {
			category:    "Auth"
			common:      true
			description: "he name of the user to connect. Default to 'root'."
			required: false
			type: string: {
				default: "root"
				examples: ["username"]
			}
		}
		password: {
			category:    "Auth"
			common:      true
			description: "The password of the user to connect. Default to ''."
			required: false
			type: string: {
				default: ""
				examples: [""]
			}
		}
	}
}
