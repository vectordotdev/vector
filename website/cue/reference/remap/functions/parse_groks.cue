package metadata

remap: functions: parse_groks: {
	category:    "Parse"
	description: """
		Parses the `value` using multiple [`grok`](\(urls.grok)) patterns. All patterns [listed here](\(urls.grok_patterns))
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
			name:        "patterns"
			description: "The [Grok patterns](https://github.com/daschl/grok/tree/master/patterns), which are tried in order until the first match."
			required:    true
			type: ["array"]
		},
		{
			name:        "aliases"
			description: "The shared set of grok aliases that can be referenced in the patterns to simplify them."
			required:    false
			default:     true
			type: ["object"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse using the provided `pattern`.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse using multiple Grok patterns"
			source: #"""
				parse_groks!(
					"2020-10-02T23:22:12.223222Z info Hello world",
					patterns: [
						"%{common_prefix} %{_status} %{_message}",
						"%{common_prefix} %{_message}",
					],
					aliases: {
						"common_prefix": "%{_timestamp} %{_loglevel}",
						"_timestamp": "%{TIMESTAMP_ISO8601:timestamp}",
						"_loglevel": "%{LOGLEVEL:level}",
						"_status": "%{POSINT:status}",
						"_message": "%{GREEDYDATA:message}"
					}
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
