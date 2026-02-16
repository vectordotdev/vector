package metadata

components: sinks: azure_event_hubs: {
	title: "Azure Event Hubs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: ["Azure"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: true
			}
			request: enabled: false
			tls: enabled: false
			to: {
				service: services.azure_event_hubs
				interface: {
					socket: {
						api: {
							title: "Azure Event Hubs AMQP protocol"
							url:   urls.azure_event_hubs
						}
						direction: "outgoing"
						port:      5671
						protocols: ["tcp"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: [
			"""
				This component uses the native Azure Event Hubs SDK over AMQP. For Kafka protocol
				compatibility, use the `kafka` sink with the
				[Event Hubs Kafka endpoint](\(urls.azure_event_hubs_kafka)).
				""",
		]
	}

	configuration: generated.components.sinks.azure_event_hubs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		authentication: {
			title: "Authentication"
			body: """
				The Azure Event Hubs sink supports two authentication modes:

				- **Connection string**: Provide a `connection_string` containing SAS credentials.
				  The sink automatically extracts the namespace, shared access key, and event hub name.
				  See the [Azure docs](\(urls.azure_event_hubs_connection_string)) for how to obtain one.

				- **Azure Identity (Managed Identity)**: Provide `namespace` and `event_hub_name` separately.
				  Authentication is handled via `ManagedIdentityCredential` from the `azure_identity` crate,
				  supporting Azure VM, AKS, and other environments with managed identity enabled.
				"""
		}
		partition_routing: {
			title: "Partition Routing"
			body: """
				By default, events are sent to Event Hubs without specifying a partition, allowing
				the service to distribute events across partitions automatically.

				To route events to specific partitions, set `partition_id_field` to a log field path
				containing the target partition ID (e.g., `.partition_id`). Events without the field
				or with an empty value are sent without partition affinity.
				"""
		}
		metrics: {
			title: "Metrics"
			body: """
				The sink emits standard Vector telemetry (`component_sent_events_total`,
				`component_sent_bytes_total`, `component_sent_event_bytes_total`) as well as
				Event Hubs-specific counters labeled by `event_hub_name` and `partition_id`:

				- `azure_event_hubs_events_sent_total` — number of events sent per partition
				- `azure_event_hubs_bytes_sent_total` — encoded bytes sent per partition
				"""
		}
	}

	telemetry: metrics: {
		component_errors_total:                components.sources.internal_metrics.output.metrics.component_errors_total
		component_sent_bytes_total:            components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_event_bytes_total:      components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		component_sent_events_total:           components.sources.internal_metrics.output.metrics.component_sent_events_total
		azure_event_hubs_events_sent_total:    components.sources.internal_metrics.output.metrics.azure_event_hubs_events_sent_total
		azure_event_hubs_bytes_sent_total:     components.sources.internal_metrics.output.metrics.azure_event_hubs_bytes_sent_total
	}
}
