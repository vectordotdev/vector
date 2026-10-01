package metadata

_schemaDefinitions: "vector::sinks::splunk_hec::common::acknowledgements::HecClientAcknowledgementsConfig": object: options: {
	enabled: {
		description: """
			Controls whether or not end-to-end acknowledgements are enabled.

			When enabled for a sink, any source that supports end-to-end
			acknowledgements that is connected to that sink waits for events
			to be acknowledged by **all connected sinks** before acknowledging them at the source.

			Enabling or disabling acknowledgements at the sink level takes precedence over any global
			[`acknowledgements`][global_acks] configuration.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			"""
		required: false
		type: bool: {}
	}
	indexer_acknowledgements_enabled: {
		description: """
			Controls if the sink integrates with [Splunk HEC indexer acknowledgements][splunk_indexer_ack_docs] for end-to-end acknowledgements.

			[splunk_indexer_ack_docs]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
			"""
		required: false
		type: bool: default: true
	}
	max_pending_acks: {
		description: """
			The maximum number of pending acknowledgements from events sent to the Splunk HEC collector.

			Once reached, the sink begins applying backpressure.
			"""
		required: false
		type: uint: default: 1000000
	}
	query_interval: {
		description: "The amount of time to wait between queries to the Splunk HEC indexer acknowledgement endpoint."
		required:    false
		type: uint: {
			default: 10
			unit:    "seconds"
		}
	}
	retry_limit: {
		description: "The maximum number of times an acknowledgement ID is queried for its status."
		required:    false
		type: uint: default: 30
	}
}
