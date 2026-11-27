package metadata

_schemaDefinitions: "core::option::Option<vector::kafka::KafkaSaslConfig>": object: options: {
	azure: {
		description: """
			Authenticate with a Microsoft Entra ID token through the `OAUTHBEARER` SASL mechanism.

			Use this to read from Azure Event Hubs through its Kafka endpoint with a managed identity or
			workload identity instead of a shared access key. Vector sets `mechanism` to `OAUTHBEARER`,
			requests tokens with `auth`, and refreshes them before they expire.

			This option enables SASL authentication when `enabled` is not set, and is ignored when
			`enabled` is `false`. It requires `tls.enabled: true`, because the token is a bearer
			credential.

			Do not set `username`, `password`, or a `mechanism` other than `OAUTHBEARER` with this
			option. In `librdkafka_options`, do not set `bootstrap.servers`, `metadata.broker.list`,
			`security.protocol`, `sasl.mechanism`, `sasl.username`, `sasl.password`,
			`sasl.oauthbearer.*`, or `enable.sasl.oauthbearer.unsecure.jwt`.

			Token requests do not use the global `proxy` option. See the "Azure Event Hubs" section under
			"How it works" for the proxy settings that they use.

			Only the `kafka` source supports this option. It requires Vector to be built with the
			`azure-core` feature.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::kafka::KafkaSaslAzureConfig>"]
	}
	enabled: {
		description: """
			Enables SASL authentication.

			If this is not set, SASL authentication is enabled only when `azure` is set.

			The `sasl.*` options configure the `PLAIN`- and `SCRAM`-based mechanisms with `username`,
			`password`, and `mechanism`, and the `OAUTHBEARER` mechanism with Microsoft Entra ID tokens
			with `azure` (`kafka` source only). For
			other mechanisms, `librdkafka_options.*` must be used directly to configure other `librdkafka`-specific values.
			If using `sasl.kerberos.*` as an example, where `*` is `service.name`, `principal`, `kinit.md`, etc., then
			`librdkafka_options.*` as a result becomes `librdkafka_options.sasl.kerberos.service.name`,
			`librdkafka_options.sasl.kerberos.principal`, etc.

			See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.

			SASL authentication is not supported on Windows.
			"""
		required: false
		type: bool: {}
	}
	mechanism: {
		description: "The SASL mechanism to use."
		required:    false
		type: string: examples: ["SCRAM-SHA-256", "SCRAM-SHA-512"]
	}
	password: {
		description: "The SASL password."
		required:    false
		type: string: examples: [
			"password"
		]
	}
	username: {
		description: "The SASL username."
		required:    false
		type: string: examples: [
			"username"
		]
	}
}
