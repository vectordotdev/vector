package metadata

_schemaDefinitions: "core::option::Option<vector::kafka::KafkaSaslConfig>": object: options: {
	enabled: {
		description: """
			Enables SASL authentication.

			Only `PLAIN`- and `SCRAM`-based mechanisms are supported when configuring SASL authentication using `sasl.*`. For
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
