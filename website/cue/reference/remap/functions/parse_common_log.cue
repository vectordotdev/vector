package metadata

remap: functions: parse_common_log: {
	category:    "Parse"
	description: """
		Parses the `value` using the [Common Log Format](\(urls.apache_common)) (CLF).
		"""
	notices: [
		"""
			Missing information in the log message may be indicated by `-`. These fields are omitted in the result.
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
			name: "timestamp_format"
			description: """
				The [date/time format](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) to use for
				encoding the timestamp.
				"""
			required: false
			default:  "%d/%b/%Y:%T %z"
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` does not match the Common Log Format.",
		"`timestamp_format` is not a valid format string.",
		"The timestamp in `value` fails to parse using the provided `timestamp_format`.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse using Common Log Format (with default timestamp format)"
			source: #"""
				parse_common_log!("127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326")
				"""#
			return: {
				host:      "127.0.0.1"
				identity:  "bob"
				user:      "frank"
				timestamp: "2000-10-10T20:55:36Z"
				message:   "GET /apache_pb.gif HTTP/1.0"
				method:    "GET"
				path:      "/apache_pb.gif"
				protocol:  "HTTP/1.0"
				status:    200
				size:      2326
			}
		},
		{
			title: "Parse using Common Log Format (with custom timestamp format)"
			source: #"""
				parse_common_log!(
					"127.0.0.1 bob frank [2000-10-10T20:55:36Z] \"GET /apache_pb.gif HTTP/1.0\" 200 2326",
					"%+"
				)
				"""#
			return: {
				host:      "127.0.0.1"
				identity:  "bob"
				user:      "frank"
				timestamp: "2000-10-10T20:55:36Z"
				message:   "GET /apache_pb.gif HTTP/1.0"
				method:    "GET"
				path:      "/apache_pb.gif"
				protocol:  "HTTP/1.0"
				status:    200
				size:      2326
			}
		},
	]
}
