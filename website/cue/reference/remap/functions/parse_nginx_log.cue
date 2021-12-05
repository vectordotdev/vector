package metadata

remap: functions: parse_nginx_log: {
	category:    "Parse"
	description: """
        Parses Nginx access and error log lines. Lines can be in [`combined`](\(urls.nginx_combined)), or [`error`](\(urls.nginx_error)) format.
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
			name:        "timestamp_format"
			description: """

				The [date/time format](\(urls.chrono_time_formats)) to use for encoding the timestamp. The time is parsed
				in local time if the timestamp doesn't specify a timezone. The default format is `%d/%b/%Y:%T %z` for
				combined logs and `%Y/%m/%d %H:%M:%S` for error logs.
				"""
			required:    false
			default:     "%d/%b/%Y:%T %z"
			type: ["string"]
		},
		{
			name:        "format"
			description: "The format to use for parsing the log."
			required:    true
			enum: {
				"combined": "Nginx combined format"
				"error":    "Default Nginx error format"
			}
			type: ["string"]
		},
	]

	internal_failure_reasons: [
		"`value` doesn't match the specified format",
		"`timestamp_format` isn't a valid format string",
		"The timestamp in `value` fails to parse using the provided `timestamp_format`",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse via Nginx log format (combined)"
			source: #"""
				parse_nginx_log!(
				    s'172.17.0.1 alice - [01/Apr/2021:12:02:31 +0000] "POST /not-found HTTP/1.1" 404 153 "http://localhost/somewhere" "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36" "2.75"',
				    "combined",
				)
				"""#
			return: {
				client:      "172.17.0.1"
				user:        "alice"
				timestamp:   "2021-04-01T12:02:31Z"
				request:     "POST /not-found HTTP/1.1"
				method:      "POST"
				path:        "/not-found"
				protocol:    "HTTP/1.1"
				status:      404
				size:        153
				referer:     "http://localhost/somewhere"
				agent:       "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36"
				compression: "2.75"
			}
		},
		{
			title: "Parse via Nginx log format (error)"
			source: #"""
				parse_nginx_log!(
				    s'2021/04/01 13:02:31 [error] 31#31: *1 open() "/usr/share/nginx/html/not-found" failed (2: No such file or directory), client: 172.17.0.1, server: localhost, request: "POST /not-found HTTP/1.1", host: "localhost:8081"',
				    "error"
				)
				"""#
			return: {
				timestamp: "2021-04-01T13:02:31Z"
				severity:  "error"
				pid:       31
				tid:       31
				cid:       1
				message:   "open() \"/usr/share/nginx/html/not-found\" failed (2: No such file or directory)"
				client:    "172.17.0.1"
				server:    "localhost"
				request:   "POST /not-found HTTP/1.1"
				host:      "localhost:8081"
			}
		},
	]
}
