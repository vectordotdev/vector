package metadata

components: sources: azure_event_hubs: {
	title: "Azure Event Hubs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		collect: from: {
			service: services.kafka
			interface: {
				socket: {
					api: {
						title: "Kafka protocol"
						url:   urls.kafka_protocol
					}
					direction: "incoming"
					port:      9093
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		connection_string: {
			description: """
				The connection string.
				See [here](\(urls.azure_event_hubs_connection)) for details.
				"""
			required:    true
			type: string: {
				examples: ["Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=RootManageSharedAccessKey;SharedAccessKey=XXXXXXXXXXXXXXXX"]
			}
		}
		namespace: {
			common:      false
			description: "The namespace name."
			required:    true
			type: string: {
				examples: ["namespace"]
				options: {}
			}
		}
		queue_name: {
			common:      false
			description: "The name of the queue to listen to."
			required:    true
			type: string: {
				examples: ["queue"]
			}
		}
		group_id: {
			common:      false
			description: "The name of the consumer group."
			required:    false
			type: string: {
				default: "$DEFAULT"
				examples: ["GROUP"]
			}
		}
	}

	how_it_works: {
		kafka: {
			title: "kafka"
			body:  """
                This component is a simple wrapper over the `kafka` source.
                See the documentation [here](\(urls.azure_event_hubs_kafka_ecosystem))
                for details on how `azure_event_hubs` can use `kafka`.
				"""
		}
	}

	telemetry: metrics: {
		kafka_queue_messages:                components.sources.internal_metrics.output.metrics.kafka_queue_messages
		kafka_queue_messages_bytes:          components.sources.internal_metrics.output.metrics.kafka_queue_messages_bytes
		kafka_requests_total:                components.sources.internal_metrics.output.metrics.kafka_requests_total
		kafka_requests_bytes_total:          components.sources.internal_metrics.output.metrics.kafka_requests_bytes_total
		kafka_responses_total:               components.sources.internal_metrics.output.metrics.kafka_responses_total
		kafka_responses_bytes_total:         components.sources.internal_metrics.output.metrics.kafka_responses_bytes_total
		kafka_produced_messages_total:       components.sources.internal_metrics.output.metrics.kafka_produced_messages_total
		kafka_produced_messages_bytes_total: components.sources.internal_metrics.output.metrics.kafka_produced_messages_bytes_total
		kafka_consumed_messages_total:       components.sources.internal_metrics.output.metrics.kafka_consumed_messages_total
		kafka_consumed_messages_bytes_total: components.sources.internal_metrics.output.metrics.kafka_consumed_messages_bytes_total
	}
}
