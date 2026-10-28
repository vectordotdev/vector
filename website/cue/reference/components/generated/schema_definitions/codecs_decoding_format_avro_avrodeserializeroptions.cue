package metadata

_schemaDefinitions: "codecs::decoding::format::avro::AvroDeserializerOptions": object: options: {
	encoding: {
		description: """
			The encoding format to use for decoding.

			Defaults to `datum` for backward compatibility.
			"""
		required: false
		type: string: {
			default: "datum"
			enum: {
				datum: """
					Single-object datum encoding.

					Each event is encoded/decoded as a standalone Avro datum without container metadata.
					This is the default encoding for backward compatibility.
					"""
				object_container_file: """
					Object Container File encoding.

					For serialization: Writes data in Avro Object Container File format, which embeds the schema
					in the output and organizes records into data blocks. Suitable for file-based storage.

					For deserialization: Reads from Avro Object Container File format, extracting multiple records.
					"""
			}
		}
	}
	max_ocf_records: {
		description: """
			Maximum number of records decoded from a single Avro Object Container File.

			Applies only when `encoding` is `object_container_file`.
			"""
		required: false
		type: uint: default: 100000
	}
	schema: {
		description: """
			The Avro schema definition.
			**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
			* `Decimal`
			* `Duration`
			* `Fixed`
			"""
		required: true
		type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
	}
	schema_source: {
		description: """
			How to handle the Avro schema for decoding.

			Defaults to `provided` for `datum` encoding and `embedded` for
			`object_container_file` encoding.
			"""
		required: false
		type: string: enum: {
			embedded: """
				Use the schema embedded in the OCF file.
				The provided schema is ignored.
				"""
			provided: """
				Use the schema provided in the configuration.
				For OCF, validate that the embedded schema matches the provided one.
				"""
		}
	}
	strip_schema_id_prefix: {
		description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
		required:    true
		type: bool: {}
	}
}
