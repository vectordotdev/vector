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
			default:     " "
			type: ["string"]
		},
	]
	return: ["map"]
	category: "Parse"
	description: """
		Parses a string in key value format.
		Fields can be delimited with a `"`. `"` within a delimited field can be escaped by `\\`.
		"""
	examples: [
		{
			title: "Successful match"
			input: {
				message: #"""
					level=info msg="Stopping all fetchers" tag=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager
					"""#
			}
			source: #"""
					. = parse_key_value(.message, field_split=" ", separator="=")
				"""#
			output: {
				level:  "info"
				msg:    "Stopping all fetchers"
				tag:    "stopping_fetchers"
				id:     "ConsumerFetcherManager-1382721708341"
				module: "kafka.consumer.ConsumerFetcherManager"
			}
		},
	]
}
