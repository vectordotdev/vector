package metadata

components: sinks: loki: {
	title: "Loki"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Grafana"]
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
				max_bytes:    1_000_000
				max_events:   100_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "snappy"
				algorithms: ["none", "gzip", "snappy"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "logfmt", "text"]
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
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.loki

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

	configuration: base.components.sinks.loki.configuration

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
				The `labels` option can be passed keys with `*` or prefixes ending with `*` to
				allow for setting multiple keys based on the contents of an object. Static keys
				override dynamically defined keys. For example, with an object:

				```json
				{
					"kubernetes": {
						"pod_labels": {
							"app": "web-server",
							"name": "unicorn"
						}
					},
					"metadata": {
						"cluster_name": "operations",
						"cluster_environment": "development",
						"cluster_version": "1.2.3"
					}
				}
				```

				and a configuration:

				```toml
				[sinks.my_sink_id.labels]
				\"pod_labels_*\" = "{{ kubernetes.pod_labels }}"
				\"*\" = "{{ metadata }}"
				cluster_name = "static_cluster_name"
				```

				this would expand into the following labels:

				```yaml
				pod_labels_app: web-server
				pod_labels_name: unicorn
				cluster_name: static_cluster_name
				cluster_environment: development
				cluster_version: 1.2.3
				"""
		}

		request_encoding: {
			title: "Request Encoding"
			body: """
				Loki can receive log entries as either protobuf or JSON requests.
				Protobuf requests are snappy compressed. JSON requests have either
				no compression or can be gzip compressed.

				For the `loki` sink this means the body will be encoded based
				on the configured `compression`.
				"""
		}
	}

	telemetry: metrics: {
		streams_total: components.sources.internal_metrics.output.metrics.streams_total
	}
}
