package metadata

components: sinks: gcp_bigquery: {
	title: "GCP BigQuery"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				max_events:   50_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["protobuf"]
				}
			}
			proxy: enabled: false
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_bigquery

				interface: {
					socket: {
						api: {
							title: "GCP BigQuery Storage Write API"
							url:   urls.gcp_bigquery
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.gcp_bigquery.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "bigquery"

			policies: [
				{
					_action: "tables.updateData"
					required_for: ["operation"]
				},
			]
		},
	]
}
