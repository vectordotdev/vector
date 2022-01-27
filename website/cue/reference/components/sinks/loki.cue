package metadata

components: sinks: loki: {
	title: "Loki"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Grafana"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    1_000_000
				max_events:   100_000
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
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
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
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

	configuration: {
		endpoint: {
			description: "The base URL of the Loki instance."
			required:    true
			type: string: {
				examples: ["http://localhost:3100"]
			}
		}
		auth: configuration._http_auth & {_args: {
			password_example: "${LOKI_PASSWORD}"
			username_example: "${LOKI_USERNAME}"
		}}
		labels: {
			description: """
				A set of labels that are attached to each batch of events. Both keys and values are templatable, which
				enables you to attach dynamic labels to events. Note: If the set of labels has high cardinality, this
				can cause drastic performance issues with Loki. To prevent this from happening, reduce the number of
				unique label keys and values.
				"""
			required: true
			type: object: {
				examples: [
					{
						"forwarder":             "vector"
						"event":                 "{{ event_field }}"
						"key":                   "value"
						"\"{{ event_field }}\"": "{{ another_event_field }}"
					},
				]
				options: {
					"*": {
						common:      false
						description: "Any Loki label, templatable"
						required:    false
						type: string: {
							default: null
							examples: ["vector", "{{ event_field }}"]
							syntax: "template"
						}
					}
				}
			}
		}
		out_of_order_action: {
			common: false
			description: """
				Some sources may generate events with timestamps that aren't in strictly chronological order. The Loki
				service can't accept a stream of such events. Vector sorts events before sending them to Loki, however
				some late events might arrive after a batch has been sent. This option specifies what Vector should do
				with those events.
				"""
			required: false
			type: string: {
				default: "drop"
				enum: {
					"drop":              "Drop the event."
					"rewrite_timestamp": "Rewrite timestamp of the event to the latest timestamp that was pushed."
				}
			}
		}
		remove_label_fields: {
			common:      false
			description: "If this is set to `true` then when labels are collected from events those fields will also get removed from the event."
			required:    false
			type: bool: default: false
		}

		remove_timestamp: {
			common:      false
			description: "If this is set to `true` then the timestamp will be removed from the event payload. Note the event timestamp will still be sent as metadata to Loki for indexing."
			required:    false
			type: bool: default: true
		}
		tenant_id: {
			common:      false
			description: """
				The tenant id that's sent with every request, by default this is not required since a proxy should set
				this header. When running Loki locally a tenant id is not required either.

				You can read more about tenant id's [here](\(urls.loki_multi_tenancy)).
				"""
			required:    false
			type: string: {
				default: null
				examples: ["some_tenant_id", "{{ event_field }}"]
				syntax: "template"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
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
