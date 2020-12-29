package metadata

remap: functions: parse_key_value: {
	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
		{
			name:        "field_split"
			description: "The string that separates the key from the value."
			required:    false
			default:     "="
			type: ["string"]
		},
		{
			name:        "separator"
			description: "The string that separates each key/value pair."
			required:    false
			default:    " "
			type: ["string"]
		},
		{
			name:        "trim_key"
			description: "Any characters that should be trimmed from around the key."
			required:    false
			type: ["string"]
		},
		{
			name:        "trim_value"
			description: "Any characters that should be trimmed from around the value."
			required:    false
			type: ["string"]
		},
	]
	return: ["map"]
	category: "Parse"
	description: """
		Parses a string in key value format.
		"""
	examples: [
		{
			title: "Successful match"
			input: {
				message: #"""
						"at":<info>,"method":<GET>,"path":</>,"protocol":<http>"
					"""#
			}
			source: #"""
					. = parse_key_value(.message, field_split=":", separator=":", trim_key="\\"", trim_value="<>")
				"""#
			output: {
				at:       "info"
				method:   "GET"
				path:     "/"
				protocol: "http"
			}
		},
	]
}
