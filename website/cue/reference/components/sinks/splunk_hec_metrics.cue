package metadata

components: sinks: splunk_hec_metrics: {
	title: "Splunk HEC metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Splunk"]
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
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: enabled: false
			proxy: enabled:    true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.splunk

				interface: {
					socket: {
						api: {
							title: "Splunk HEC event endpoint"
							url:   urls.splunk_hec_event_endpoint
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
		notices: []
	}

	configuration: base.components.sinks.splunk_hec_metrics.configuration

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

	telemetry: components.sinks.splunk_hec_logs.telemetry

	how_it_works: sinks._splunk_hec.how_it_works & {
		multi_value_tags: {
			title: "Multivalue Tags"
			body: """
				If Splunk receives a tag with multiple values it will only take the last value specified,
				so Vector only sends this last value.
				"""
		}}
}
