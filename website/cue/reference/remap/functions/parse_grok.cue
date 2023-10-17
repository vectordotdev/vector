package metadata

remap: functions: parse_grok: {
	category:    "Parse"
	description: """
		Parses the `value` using the [`grok`](\(urls.grok)) format. All patterns [listed here](\(urls.grok_patterns))
		are supported.
		"""
	notices: [
		"""
			We recommend using community-maintained Grok patterns when possible, as they're more likely to be properly
			vetted and improved over time than bespoke patterns.
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
			name:        "pattern"
			description: "The [Grok pattern](https://github.com/daschl/grok/tree/master/patterns)."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse using the provided `pattern`.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse using Grok"
			source: #"""
				parse_grok!(
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
