package metadata

_schemaDefinitions: "codecs::encoding::format::protobuf::ProtobufSerializerOptions": object: options: {
	desc_file: {
		description: """
			The path to the protobuf descriptor set file.

			This file is the output of `protoc -I <include path> -o <desc output path> <proto>`

			You can read more [here](https://buf.build/docs/reference/images/#how-buf-images-work).
			"""
		required: true
		type: string: examples: ["/etc/vector/protobuf_descriptor_set.desc"]
	}
	message_type: {
		description: "The name of the message type to use for serializing."
		required:    true
		type: string: examples: ["package.Message"]
	}
	use_json_names: {
		description: """
			Use JSON field names (camelCase) instead of protobuf field names (snake_case).

			When enabled, the serializer looks for fields using their JSON names as defined
			in the `.proto` file (for example `jobDescription` instead of `job_description`).

			This is useful when working with data that has already been converted from JSON or
			when interfacing with systems that use JSON naming conventions.
			"""
		required: false
		type: bool: default: false
	}
}
