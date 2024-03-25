package metadata

remap: functions: parse_proto: {
	category: "Parse"
	description: """
		Parses the `value` as a protocol buffer payload.
		"""
	notices: [
		"""
				Only proto messages are parsed and returned.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The protocol buffer payload to parse."
			required:    true
			type: ["string"]
		},
		{
			name: "desc_file"
			description: """
				The path to the protobuf descriptor set file. Must be a literal string.

				This file is the output of protoc -o <path> ...
				"""
			required: true
			type: ["string"]
		},
		{
			name: "message_type"
			description: """
				The name of the message type to use for serializing.

				Must be a literal string.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid proto payload.",
		"`desc_file` file does not exist.",
		"`message_type` message type does not exist in the descriptor file.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse proto"
			source: #"""
				parse_proto!(decode_base64!("Cgdzb21lb25lIggKBjEyMzQ1Ng=="), "resources/protobuf_descriptor_set.desc", "test_protobuf.Person")
				"""#
			return: {
				name: "someone"
				phones: [
					{
						number: "123456"
					},
				]
			}
		},
	]
}
