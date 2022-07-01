package metadata

components: sinks: apex: {
	title: "Apex"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["apex.sh"]
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    1_000_000
				max_events:   100_000
				timeout_secs: 1.0
			}
            compression: {
				enabled: false
			}
			encoding: {
				enabled: false
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
			}
			to: {
				service: services.apex

				interface: {
					socket: {
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

	configuration: {
		uri: {
			description: "The base URL of the Apex instance. Vector will append `/add_events` to this."
			required:    true
			type: string: {
				examples: ["http://localhost:3100"]
			}
		}
		project_id: {
			description: "The id of the project in the Apex instance."
			required:    true
			type: string: {
				examples: ["my-project"]
			}
		}
        api_token: {
			description: "The api token to use to authenticate with Apex."
			required:    true
			type: string: {
				examples: ["${API_TOKEN}"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		decentralized_deployments: {
			title: "Decentralized Deployments"
			body: """
				Loki currently does not support out-of-order inserts. If
				Vector is deployed in a decentralized setup then there is
				the possibility that logs might get rejected due to data
				races between Vector instances. To avoid this we suggest
				either assigning each Vector instance with a unique label
				or deploying a centralized Vector which will ensure no logs
				will get sent out-of-order.
				"""
		}

		event_ordering: {
			title: "Event Ordering"
			body: """
				The `loki` sink will ensure that all logs are sorted via
				their `timestamp`. This is to ensure that logs will be
				accepted by Loki. If no timestamp is supplied with events
				then the Loki sink will supply its own monotonically
				increasing timestamp.
				"""
		}

		label_expansion: {
			title: "Label Expansion"
			body: """
				The `labels` option can be passed keys suffixed with "*" to
				allow for setting multiple keys based on the contents of an
				object. For example, with an object:

				```json
				{"kubernetes":{"pod_labels":{"app":"web-server","name":"unicorn"}}}
				```

				and a configuration:

				```toml
				[sinks.my_sink_id.labels]
				pod_labels_*: "{{ kubernetes.pod_labels }}"
				```

				This would expand into two labels:

				```toml
				pod_labels_app: web-server
				pod_labels_name: unicorn
				"""
		}
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
		streams_total:                    components.sources.internal_metrics.output.metrics.streams_total
	}
}
