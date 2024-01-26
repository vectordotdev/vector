package metadata

remap: functions: parse_key_value: {
	category:    "Parse"
	description: """
		Parses the `value` in key-value format. Also known as [logfmt](\(urls.logfmt)).

		* Keys and values can be wrapped with `"`.
		* `"` characters can be escaped using `\\`.
		"""
	notices: [
		"""
			All values are returned as strings or as an array of strings for duplicate keys. We recommend manually coercing values to desired types as you see fit.
			""",
	]

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
			description: "The string that separates each key-value pair."
			required:    false
			default:     " "
			type: ["string"]
		},
		{
			name:        "whitespace"
			description: "Defines the acceptance of unnecessary whitespace surrounding the configured `key_value_delimiter`."
			required:    false
			enum: {
				lenient: "Ignore whitespace."
				strict:  "Parse whitespace as normal character."
			}
			default: "lenient"
			type: ["string"]
		},
		{
			name:        "accept_standalone_key"
			description: "Whether a standalone key should be accepted, the resulting object associates such keys with the boolean value `true`."
			required:    false
			type: ["boolean"]
			default: true
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted key-value string.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse logfmt log"
			source: #"""
				parse_key_value!(
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
				parse_key_value!(
					"path:\"/cart_link\", host:store.app.com, fwd: \"102.30.171.16\", dyno: web.1, connect:0ms, service:87ms, status:304, bytes:632, protocol:https",
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
		{
			title: "Parse comma delimited log with standalone keys"
			source: #"""
				parse_key_value!(
					"env:prod,service:backend,region:eu-east1,beta",
					field_delimiter: ",",
					key_value_delimiter: ":",
				)
				"""#
			return: {
				env:     "prod"
				service: "backend"
				region:  "eu-east1"
				beta:    true
			}
		},
		{
			title: "Parse duplicate keys"
			source: #"""
				parse_key_value!(
					"at=info,method=GET,path=\"/index\",status=200,tags=dev,tags=dummy",
					field_delimiter: ",",
					key_value_delimiter: "=",
				)
				"""#
			return: {
				at:     "info"
				method: "GET"
				path:   "/index"
				status: "200"
				tags: ["dev", "dummy"]
			}
		},
	]
}
