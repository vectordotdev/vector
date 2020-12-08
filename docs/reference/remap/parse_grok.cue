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
	]
	return: ["map"]
	category: "parse"
	description: #"""
		Parses a string using the Rust [`grok` library](https://github.com/daschl/grok). All patterns
		[listed here](https://github.com/daschl/grok/tree/master/patterns) are supported. It is recommended
		to use maintained patterns when possible since they will be improved over time by the community.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: "2020-10-02T23:22:12.223222Z info Hello world"
			}
			source: #"""
					.grokked = parse_grok(.message, "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}")
				"""#
			output: {
				message:             "2020-10-02T23:22:12.223222Z info Hello world"
				"grokked.timestamp": "2020-10-02T23:22:12.223222Z"
				"grokked.level":     "info"
				"grokked.message":   "Hello world"
			}
		},
	]
}
