package metadata

components: sinks: opsimate: {
	title: "OpsiMate"

	classes: {
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["OpsiMate"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				max_events:   1
				timeout_secs: 1.0
			}
			compression: {
				enabled: false
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.opsimate

				interface: {
					socket: {
						api: {
							title: "OpsiMate custom alerts API"
							url:   urls.opsimate_custom_alerts
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
	}

	configuration: generated.components.sinks.opsimate.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Find the API token of your OpsiMate server: the value of its `API_TOKEN` setting
				   (`opsimate` in a default installation).

				2. Point the sink at your OpsiMate server with `endpoint` (the base URL, for example
				   `http://opsimate.example.com:3001`) and pass the token as `api_token`. Alerts are sent to
				   `{endpoint}/api/v1/alerts/custom` and the health check queries `{endpoint}/health`.

				See the [OpsiMate Vector integration guide](\(urls.opsimate_vector_integration)) for a
				complete walkthrough.
				"""
		}

		configuration: {
			title: "Configuration"
			body:  """
				Each log event is sent as-is, after the `encoding` transformer has been applied, as the
				JSON body of one request. OpsiMate's custom alerts endpoint accepts a single alert object per
				request, so `batch.max_events` is fixed to `1` and any other value is rejected at startup.

				The event must carry the fields OpsiMate requires for a custom alert, and may carry the
				optional ones:

				* `id` (required): a stable identifier for the alert, used to update or resolve it later.
				* `alertName` (required): the human-readable name of the alert.
				* `tags` (required): an object of string key/value pairs, such as `{"service": "checkout", "host": "web-01"}`,
				  used for filtering, grouping and enrichment rules.
				* `summary`, `severity`, `status`, `startsAt`, `team`, `links` (optional).

				Shape the payload upstream with a `remap` transform, for example:

				```yaml
				transforms:
				  to_opsimate_alert:
				    type: remap
				    inputs: ["my_source"]
				    source: |
				      . = {
				        "id": .alert_id,
				        "alertName": .name,
				        "tags": ["vector", .service],
				        "summary": .message,
				        "severity": .severity,
				        "status": "firing",
				        "startsAt": format_timestamp!(.timestamp, "%+")
				      }
				```

				See the [custom alerts reference](\(urls.opsimate_custom_alerts)) for the full payload
				contract.
				"""
		}
	}
}
