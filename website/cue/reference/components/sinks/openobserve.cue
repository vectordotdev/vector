package metadata

components: sinks: openobserve: {
	title: "OpenObserve"

	features: {
		healthcheck: {
			enabled: false
		}
		send: {
			compression: {
				enabled: true
				default: "gzip"
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: "json"
				}
				timestamp_format: {
					enabled: true
					default: "rfc3339"
				}
			}
		}
	}

	configuration: {
		type: "http"
		inputs: ["source_or_transform_id"]
		uri: {
			description: "The OpenObserve endpoint to send data to."
			required: true
			type: string: default: "http://localhost:5080/api/default/default/_json"
		}
		method: {
			description: "The HTTP method to use."
			required: true
			type: string: default: "post"
		}
		auth: {
			strategy: {
				description: "The authentication strategy."
				required: true
				type: string: default: "basic"
			}
			user: {
				description: "The username for basic authentication."
				required: true
				type: string: default: "test@example.com"
			}
			password: {
				description: "The password for basic authentication."
				required: true
				type: string: default: "your_ingestion_password"
			}
		}
	}
}
