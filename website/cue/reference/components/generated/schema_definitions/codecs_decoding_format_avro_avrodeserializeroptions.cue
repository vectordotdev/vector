package metadata

_schemaDefinitions: "codecs::decoding::format::avro::AvroDeserializerOptions": object: options: {
	schema: {
		description: """
			The Avro schema definition.
			**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
			* `Date`
			* `Decimal`
			* `Duration`
			* `Fixed`
			* `TimeMillis`
			"""
		required: true
		type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
	}
	strip_schema_id_prefix: {
		description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
		required:    true
		type: bool: {}
	}
}
