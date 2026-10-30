package metadata

components: sinks: gcp_bigquery: {
	title: "GCP BigQuery"

	classes: {
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
			proxy: enabled: true
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
	}

	configuration: generated.components.sinks.gcp_bigquery.configuration

	how_it_works: {
		healthcheck_permissions: {
			title: "Healthcheck permissions"
			body: """
				Health checking calls the `GetWriteStream` API, which requires the
				`bigquery.tables.get` permission. Writing rows only requires
				`bigquery.tables.updateData`. If your service account has write-only
				permissions, disable the healthcheck with `healthcheck.enabled = false`.
				"""
		}
		self_contained_schema: {
			title: "Self-contained Protobuf schema"
			body: """
				Vector sends BigQuery the descriptor of the configured `message_type`, so
				that message must be self-contained, meaning that every message or enum type
				used by one of its fields must be defined as a nested type inside it.
				References to other top-level messages, including types such as
				`google.protobuf.Timestamp`, are rejected when the sink starts.

				The message must also be defined with proto2 syntax.
				BigQuery expects rows encoded with proto2 semantics, and proto3 omits fields
				set to their default value (such as `0` or an empty string), which BigQuery
				would store as `NULL`. Messages from proto3 files are rejected when the sink
				starts.
				"""
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  true
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "bigquery"

			policies: [
				{
					_action: "tables.get"
					required_for: ["healthcheck"]
				},
				{
					_action: "tables.updateData"
					required_for: ["operation"]
				},
			]
		},
	]
}
