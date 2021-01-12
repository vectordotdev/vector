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
			title: "Parse (logfmt)"
			input: log: message: #"""
				@timestamp="Sun Jan 10 16:47:39 EST 2021" level=info msg="Stopping all fetchers" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager
				"""#
			source: #"""
				. = parse_key_value(del(.message))
				"""#
			output: log: {
				"@timestamp":     "Sun Jan 10 16:47:39 EST 2021"
				level:            "info"
				msg:              "Stopping all fetchers"
				"tag#production": "stopping_fetchers"
				id:               "ConsumerFetcherManager-1382721708341"
				module:           "kafka.consumer.ConsumerFetcherManager"
			}
		},
		{
			title: "Parse (comma delimited)"
			input: log: message: #"""
				path:"/cart_link", host:store.app.com, fwd: "102.30.171.16", dyno: web.1 connect:0ms, service:87ms, status:304, bytes:632, protocol:https
				"""#
			source: #"""
				. = parse_key_value(del(.message), field_delimiter: ",", key_value_delimiter: ":")
				"""#
			output: log: {
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
