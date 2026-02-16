package metadata

components: sources: azure_event_hubs: {
	title: "Azure Event Hubs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.azure_event_hubs
				interface: {
					socket: {
						api: {
							title: "Azure Event Hubs AMQP protocol"
							url:   urls.azure_event_hubs
						}
						direction: "incoming"
						port:      5671
						protocols: ["tcp"]
						ssl: "required"
					}
				}
			}
		}
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: [
			"""
				This component uses the native Azure Event Hubs SDK over AMQP. For Kafka protocol
				compatibility, use the `kafka` source with the
				[Event Hubs Kafka endpoint](\(urls.azure_event_hubs_kafka)).
				""",
		]
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.azure_event_hubs.configuration

	output: {
		logs: record: {
			description: "An individual Azure Event Hubs event."
			fields: {
				message: {
					description: "The raw body from the Event Hubs event."
					required:    true
					type: string: {
						examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
						syntax: "literal"
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["azure_event_hubs"]
					}
				}
				partition_id: {
					description: "The partition ID from which the event was received."
					required:    true
					type: string: {
						examples: ["0", "1"]
					}
				}
				sequence_number: {
					description: "The sequence number assigned by Event Hubs."
					required:    false
					type: uint: {
						examples: [100]
						unit: null
					}
				}
				offset: {
					description: "The offset of the event within the partition."
					required:    false
					type: string: {
						examples: ["1024"]
						syntax: "literal"
					}
				}
				timestamp: fields._current_timestamp & {
					description: "The timestamp when the event was ingested by Vector."
				}
			}
		}
		metrics: "": {
			description: "Metric events that may be emitted by this source."
		}
		traces: "": {
			description: "Trace events that may be emitted by this source."
		}
	}

	telemetry: metrics: {
		component_errors_total:           components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:   components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:  components.sources.internal_metrics.output.metrics.component_received_events_total
	}

	how_it_works: {
		authentication: {
			title: "Authentication"
			body: """
				The Azure Event Hubs source supports two authentication modes:

				- **Connection string**: Provide a `connection_string` containing SAS credentials.
				  The source automatically extracts the namespace, shared access key, and event hub name.
				  See the [Azure docs](\(urls.azure_event_hubs_connection_string)) for how to obtain one.

				- **Azure Identity (Managed Identity)**: Provide `namespace` and `event_hub_name` separately.
				  Authentication is handled via `ManagedIdentityCredential` from the `azure_identity` crate,
				  supporting Azure VM, AKS, and other environments with managed identity enabled.
				"""
		}
		partitions: {
			title: "Multi-Partition Consumption"
			body: """
				By default, the source auto-discovers all partitions via the Event Hub properties API
				and spawns a dedicated receiver task per partition. You can restrict consumption to
				specific partitions by setting `partition_ids` to a list of partition ID strings
				(e.g., `["0", "1"]`).

				Each partition receiver reconnects automatically with exponential backoff on failures.
				"""
		}
	}
}
