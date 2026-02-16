package metadata

generated: components: sources: azure_event_hubs: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	connection_string: {
		description: """
			The connection string for the Event Hubs namespace.

			Must include `Endpoint`, `SharedAccessKeyName`, and `SharedAccessKey`.
			Optionally includes `EntityPath` for the Event Hub name.

			If not set, authentication falls back to `azure_identity` (e.g., Managed Identity).
			In that case, `namespace` and `event_hub_name` must be provided.
			"""
		required: false
		type: string: {
			examples: ["Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"]
		}
	}
	namespace: {
		description: """
			The fully qualified Event Hubs namespace host.

			Required when not using a connection string.
			For example: `mynamespace.servicebus.windows.net`.
			"""
		required: false
		type: string: {
			examples: ["mynamespace.servicebus.windows.net"]
		}
	}
	event_hub_name: {
		description: """
			The name of the Event Hub to consume from.

			Required if the connection string does not include `EntityPath`.
			"""
		required: false
		type: string: {
			examples: ["my-event-hub"]
		}
	}
	consumer_group: {
		description: "The consumer group to use."
		required:    false
		type: string: {
			default: "$Default"
			examples: ["$Default", "my-consumer-group"]
		}
	}
	partition_ids: {
		description: """
			The partition IDs to consume from.

			If empty or not specified, all partitions are consumed automatically.
			Provide specific IDs to consume a subset.
			"""
		required: false
		type: array: items: type: string: {
			examples: ["0", "1"]
		}
	}
	start_position: {
		description: """
			Where to start reading events from.

			Possible values: `latest`, `earliest`.
			"""
		required: false
		type: string: {
			default: "latest"
			examples: ["latest", "earliest"]
		}
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type: object: options: {}
	}
	decoding: {
		description: "Decoding configuration."
		required:    false
		type: object: options: {}
	}
}
