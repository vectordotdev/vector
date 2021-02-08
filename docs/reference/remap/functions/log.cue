package metadata

remap: functions: log: {
	category:    "Debug"
	description: """
		Logs the `value` to Vector's [stdout](\(urls.stdout)) at the specified `level`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to log."
			required:    true
			type: ["any"]
		},
		{
			name:        "level"
			description: "The log level."
			required:    false
			type: ["string"]
			enum: {
				trace: "Log at the `trace` level."
				debug: "Log at the `debug` level."
				info:  "Log at the `info` level."
				warn:  "Log at the `warn` level."
				error: "Log at the `error` level."
			}
			default: "info"
		},
	]
	internal_failure_reasons: []
	return: types: ["null"]

	examples: [
		{
			title: "Log a message"
			source: #"""
				log("Hello, World!", level: "info")
				"""#
			return: null
		},
		{
			title: "Log an error"
			source: #"""
				ts, err = format_timestamp(to_timestamp("10-Oct-2020 1"))
				if err != null {
					log(err, level: "error")
				}
				"""#
			return: null
		},
	]
}
