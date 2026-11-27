package metadata

_schemaDefinitions: "core::option::Option<vector::kafka::KafkaSaslAzureConfig>": object: options: {
	auth: {
		description: "The Azure credential to request tokens with."
		required:    true
		type:        _schemaDefinitions["vector::sinks::azure_common::config::AzureAuthentication"]
	}
	token_scope: {
		description: """
			The scope to request tokens for.

			For Azure Event Hubs, use `https://<namespace>.servicebus.windows.net/.default`, where
			`<namespace>` is the Event Hubs namespace in `bootstrap_servers`. Vector sends the token to
			the brokers in `bootstrap_servers` and to the brokers that they advertise, so name only that
			namespace, never a broader resource.
			"""
		required: true
		type: string: examples: ["https://mynamespace.servicebus.windows.net/.default"]
	}
}
