package metadata

components: sources: azure_event_hubs: {
	title: "Azure Event Hubs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar", "aggregator"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		collect: {
			checkpoint: enabled: false
			tls: {
				enabled:                true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
			}
			from: components._kafka.features.collect.from
		}
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		acknowledgements: configuration._source_acknowledgements
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
			description: "The namespace name."
			required:    true
			type: string: {
				examples: ["namespace"]
			}
		}
		queue_name: {
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
				This component leverages event hubs' compatability with `kafka`.
				See the documentation [here](\(urls.azure_event_hubs_kafka_ecosystem))
				for details on how `azure_event_hubs` can use `kafka`.
				"""
		}
	}

	telemetry: metrics: {
		events_failed_total:                  components.sources.internal_metrics.output.metrics.events_failed_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		consumer_offset_updates_failed_total: components.sources.internal_metrics.output.metrics.consumer_offset_updates_failed_total
		kafka_queue_messages:                 components.sources.internal_metrics.output.metrics.kafka_queue_messages
		kafka_queue_messages_bytes:           components.sources.internal_metrics.output.metrics.kafka_queue_messages_bytes
		kafka_requests_total:                 components.sources.internal_metrics.output.metrics.kafka_requests_total
		kafka_requests_bytes_total:           components.sources.internal_metrics.output.metrics.kafka_requests_bytes_total
		kafka_responses_total:                components.sources.internal_metrics.output.metrics.kafka_responses_total
		kafka_responses_bytes_total:          components.sources.internal_metrics.output.metrics.kafka_responses_bytes_total
		kafka_produced_messages_total:        components.sources.internal_metrics.output.metrics.kafka_produced_messages_total
		kafka_produced_messages_bytes_total:  components.sources.internal_metrics.output.metrics.kafka_produced_messages_bytes_total
		kafka_consumed_messages_total:        components.sources.internal_metrics.output.metrics.kafka_consumed_messages_total
		kafka_consumed_messages_bytes_total:  components.sources.internal_metrics.output.metrics.kafka_consumed_messages_bytes_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
	}
}
