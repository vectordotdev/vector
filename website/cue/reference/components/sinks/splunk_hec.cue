package metadata

components: sinks: _splunk_hec: {
	configuration: {
		acknowledgements: {
			common:      false
			description: "The configuration for Splunk HEC indexer acknowledgement client behavior."
			required:    false
			type: object: {
				options: {
					indexer_acknowledgements_enabled: {
						common:      false
						description: "Controls if the sink will integrate with [Splunk HEC indexer acknowledgements](\(urls.splunk_hec_indexer_acknowledgements)) for end-to-end acknowledgements."
						required:    false
						type: bool: {
							default: true
						}
					}
					query_interval: {
						common:      false
						description: "The amount of time to wait in between queries to the Splunk HEC indexer acknowledgement endpoint. Minimum of `1`."
						required:    false
						type: uint: {
							default: 10
							unit:    "seconds"
						}
					}
					retry_limit: {
						common:      false
						description: "The maximum number of times an ack id will be queried for its status. Minimum of `1`."
						required:    false
						type: uint: {
							default: 30
							unit:    null
						}
					}
					max_pending_acks: {
						common:      false
						description: "The maximum number of ack ids pending query. Once reached, the sink will begin applying backpressure."
						required:    false
						type: uint: {
							default: 1_000_000
							unit:    null
						}
					}
				}
			}
		}
	}
	how_it_works: {
		indexer_acknowledgements: {
			title: "Indexer Acknowledgements"
			body:  """
				To provide more accurate end-to-end acknowledgements, this sink will automatically integrate (unless explicitly disabled) with
				[Splunk HEC indexer acknowledgements](\(urls.splunk_hec_indexer_acknowledgements))
				if the provided Splunk HEC token has the feature enabled. In other words, if `ackID`'s are present in Splunk
				HEC responses, this sink will store and query for the status of said `ackID`'s to confirm that data has been successfully
				delivered. Upstream sources with the Vector end-to-end acknowledgements feature enabled will wait for this sink to confirm
				delivery of events before acknowledging receipt.

				The Splunk channel required for indexer acknowledgements is created using a randomly generated UUID. By default, this sink uses the
				recommended Splunk indexer acknowledgements client behavior: querying for ack statuses every 10 seconds for a maximum of 30 attempts
				(5 minutes) per `ackID`.
				"""
		}
	}
}
