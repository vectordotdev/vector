package metadata

components: sinks: _splunk_hec: {
	configuration: {
		acknowledgements: {
			common:      false
			description: "The configuration for Splunk HEC indexer acknowledgement client behavior."
			required:    false
			type: object: {
				options: {
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
				}
			}
		}
	}
	how_it_works: {
		indexer_acknowledgements: {
			title: "Indexer Acknowledgements"
			body: """
				For more accurate end-to-end acknowledgements, this sink will automatically integrate with
				[Splunk HEC indexer acknowledgements](https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck)
				if the provided Splunk HEC token has the feature enabled. In other words, if `ackID`'s are present in Splunk
				HEC responses, this sink will store and query for the status of said `ackID`'s to confirm that data has been successfully
				delivered.

				The Splunk channel required for indexer acknowledgements is created using a randomly generated UUID.
				"""
		}
	}
}
