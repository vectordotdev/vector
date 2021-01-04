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
			name:        "key_value_delimiter"
			description: "The string that separates the key from the value."
			required:    false
			default:     "="
			type: ["string"]
		},
		{
			name:        "field_delimiter"
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
		Keys and values can be wrapped with a `"`. `"` characters within a delimited field can be escaped by `\\`.
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
					. = parse_key_value(.message)
				"""#
			output: {
				level:  "info"
				msg:    "Stopping all fetchers"
				tag:    "stopping_fetchers"
				id:     "ConsumerFetcherManager-1382721708341"
				module: "kafka.consumer.ConsumerFetcherManager"
			}
		},
		{
			title: "Custom delimiters"
			input: {
				message: #"""
					path:"/cart_link", host:store.app.com, fwd: "102.30.171.16", dyno: web.1 connect:0ms, service:87ms, status:304, bytes:632, protocol:https
					"""#
			}
			source: #"""
					. = parse_key_value(.message, field_delimiter=",", key_value_delimiter=":")
				"""#
			output: {
				path:     "/cart_link"
				host:     "store.app.com"
				fwd:      "102.30.171.16"
				dyno:     "web.1"
				connect:  "0ms"
				service:  "87ms"
				status:   "304"
				bytes:    "632"
				protocol: "https"
			}
		},
	]
}
