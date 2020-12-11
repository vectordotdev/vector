package metadata

remap: functions: to_severity: {
	arguments: [
		{
			name:        "level"
			description: "The Syslog level keyword to convert."
			required:    true
			type: ["string"]
		}
	]

	return: ["integer"]
	category:    "parse"
	description: """
		Converts a Syslog [log level keyword](\(urls.syslog_levels)) into an integer severity level
		(0 to 7). Throws an error if the level isn't recognized. The now-deprecated keywords
		`panic`, `error`, and `warn` are converted to `0`, `3`, and `4` respectively.
		"""

	examples: [
		{
			title: "Success"
			input: {
				string: "alert"
			}
			source: ".severity = to_severity(.log_level)"
			output: {
				integer: 1
			}
		},
	]
}
