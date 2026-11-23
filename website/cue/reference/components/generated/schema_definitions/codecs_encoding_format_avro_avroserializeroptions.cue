package metadata

_schemaDefinitions: "codecs::encoding::format::avro::AvroSerializerOptions": object: options: schema: {
	description: "The Avro schema."
	required:    true
	type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
}
