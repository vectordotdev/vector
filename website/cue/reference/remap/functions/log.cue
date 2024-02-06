package metadata

remap: functions: log: {
	category:    "Debug"
	description: """
		Logs the `value` to [stdout](\(urls.stdout)) at the specified `level`.
		"""

	pure: false

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
		{
			name: "rate_limit_secs"
			description: #"""
				Specifies that the log message is output no more than once per the given number of seconds.
				Use a value of `0` to turn rate limiting off.
				"""#
			type: ["integer"]
			required: false
			default:  1
		},
	]
	internal_failure_reasons: []
	return: types: ["null"]

	examples: [
		{
			title: "Log a message"
			source: #"""
				log("Hello, World!", level: "info", rate_limit_secs: 60)
				"""#
			return: null
		},
		{
			title: "Log an error"
			input: log: field: "not an integer"
			source: #"""
				_, err = to_int(.field)
				if err != null {
					log(err, level: "error")
				}
				"""#
			return: null
		},
	]
}
