package metadata

_schemaDefinitions: "derived::535994dff55dd58163f1867e": object: options: {
	desc_file: {
		description: """
			The path to the protobuf descriptor set file.

			This file is the output of `protoc -I <include path> -o <desc output path> <proto>`.

			For more information, see [How Buf images work](https://buf.build/docs/reference/images/#how-buf-images-work).
			"""
		required: false
		type: string: default: ""
	}
	message_type: {
		description: "The name of the message type to use for serializing."
		required:    false
		type: string: {
			default: ""
			examples: ["package.Message"]
		}
	}
	use_json_names: {
		description: """
			Use JSON field names (camelCase) instead of protobuf field names (snake_case).

			When enabled, the deserializer will output fields using their JSON names as defined
			in the `.proto` file (for example, `jobDescription` instead of `job_description`).

			This is useful when working with data that needs to be converted to JSON or
			when interfacing with systems that use JSON naming conventions.
			"""
		required: false
		type: bool: default: false
	}
}
