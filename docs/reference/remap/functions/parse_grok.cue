package metadata

remap: functions: parse_grok: {
	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The [Grok pattern](https://github.com/daschl/grok/tree/master/patterns)."
			required:    true
			type: ["string"]
		},
		{
			name:        "remove_empty"
			description: "If set to true, any patterns that resolve to an empty value will be removed from the result."
			required:    false
			default:     true
			type: ["boolean"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse via the provided `pattern`",
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses the provided `value` using the Rust [`grok` library](https://github.com/daschl/grok).

		All patterns [listed here](https://github.com/daschl/grok/tree/master/patterns) are supported.
		It is recommended to use maintained patterns when possible since they will be improved over time
		by the community.
		"""#
	examples: [
		{
			title: "Parse via Grok"
			source: #"""
				parse_grok(
					"2020-10-02T23:22:12.223222Z info Hello world",
					"%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
				)
				"""#
			return: {
				timestamp: "2020-10-02T23:22:12.223222Z"
				level:     "info"
				message:   "Hello world"
			}
		},
	]
}
