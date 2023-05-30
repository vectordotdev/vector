package metadata

components: sinks: gcp_stackdriver_metrics: {
	title: "GCP Cloud Monitoring (formerly Stackdriver) Metrics"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: false
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   1
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled:        true
				rate_limit_num: 1000
				headers:        false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_cloud_monitoring

				interface: {
					socket: {
						api: {
							title: "REST Interface"
							url:   urls.gcp_stackdriver_metrics_rest
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

	configuration: base.components.sinks.gcp_stackdriver_metrics.configuration

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: false
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	how_it_works: {
		duplicate_tags_names: {
			title: "Duplicate tag names"
			body: """
					Multiple tags with the same name cannot be sent to GCP. Vector will only send
					the last value for each tag name.
				"""
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "monitoring"

			policies: [
				{
					_action: "timeSeries.create"
					required_for: ["healthcheck", "operation"]
				},
			]
		},
	]
}
