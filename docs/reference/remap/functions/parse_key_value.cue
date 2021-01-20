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
	internal_failure_reasons: [
		"`value` is not a properly formatted key/value string",
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses the provided `value` in key value format. Also known as [logfmt](\(urls.logfmt)).

		* Keys and values can be wrapped with `"`.
		* `"` characters can be escaped by `\`.
		"""#
	examples: [
		{
			title: "Parse logfmt log"
			source: #"""
				parse_key_value(
					"@timestamp=\"Sun Jan 10 16:47:39 EST 2021\" level=info msg=\"Stopping all fetchers\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"
				)
				"""#
			return: {
				"@timestamp":     "Sun Jan 10 16:47:39 EST 2021"
				level:            "info"
				msg:              "Stopping all fetchers"
				"tag#production": "stopping_fetchers"
				id:               "ConsumerFetcherManager-1382721708341"
				module:           "kafka.consumer.ConsumerFetcherManager"
			}
		},
		{
			title: "Parse comma delimited log"
			source: #"""
				parse_key_value(
					"path:\"/cart_link\", host:store.app.com, fwd: \"102.30.171.16\", dyno: web.1 connect:0ms, service:87ms, status:304, bytes:632, protocol:https",
					field_delimiter: ",",
					key_value_delimiter: ":"
				)
				"""#
			return: {
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
