package metadata

remap: functions: parse_nginx_log: {
	category:    "Parse"
	description: """
      Parses Nginx access and error log lines. Lines can be in [`combined`](\(urls.nginx_combined)),
      [`ingress_upstreaminfo`](\(urls.nginx_ingress_upstreaminfo)), or [`error`](\(urls.nginx_error)) format.
      """
	notices: [
		"""
			Missing information in the log message may be indicated by `-`. These fields are omitted in the result.
			""",
		"""
			In case of `ingress_upstreaminfo` format the following fields may be safely omitted in the log message: `remote_addr`, `remote_user`, `http_referer`, `http_user_agent`, `proxy_alternative_upstream_name`, `upstream_addr`, `upstream_response_length`, `upstream_response_time`, `upstream_status`.
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
				"combined":             "Nginx combined format"
				"error":                "Default Nginx error format"
				"ingress_upstreaminfo": "Provides detailed upstream information (Nginx Ingress Controller)"
			}
			type: ["string"]
		},
	]

	internal_failure_reasons: [
		"`value` does not match the specified format.",
		"`timestamp_format` is not a valid format string.",
		"The timestamp in `value` fails to parse using the provided `timestamp_format`.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse via Nginx log format (combined)"
			source: #"""
				parse_nginx_log!(
				    s'172.17.0.1 - alice [01/Apr/2021:12:02:31 +0000] "POST /not-found HTTP/1.1" 404 153 "http://localhost/somewhere" "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36" "2.75"',
				    "combined",
				)
				"""#
			return: {
				agent:       "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36"
				client:      "172.17.0.1"
				compression: "2.75"
				referer:     "http://localhost/somewhere"
				request:     "POST /not-found HTTP/1.1"
				size:        153
				status:      404
				timestamp:   "2021-04-01T12:02:31Z"
				user:        "alice"
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
		{
			title: "Parse via Nginx log format (ingress_upstreaminfo)"
			source: #"""
				parse_nginx_log!(
				    s'0.0.0.0 - bob [18/Mar/2023:15:00:00 +0000] "GET /some/path HTTP/2.0" 200 12312 "https://10.0.0.1/some/referer" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/111.0.0.0 Safari/537.36" 462 0.050 [some-upstream-service-9000] [some-other-upstream-5000] 10.0.50.80:9000 19437 0.049 200 752178adb17130b291aefd8c386279e7',
				    "ingress_upstreaminfo"
				)
				"""#
			return: {
				body_bytes_size:                 12312
				http_referer:                    "https://10.0.0.1/some/referer"
				http_user_agent:                 "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/111.0.0.0 Safari/537.36"
				proxy_alternative_upstream_name: "some-other-upstream-5000"
				proxy_upstream_name:             "some-upstream-service-9000"
				remote_addr:                     "0.0.0.0"
				remote_user:                     "bob"
				req_id:                          "752178adb17130b291aefd8c386279e7"
				request:                         "GET /some/path HTTP/2.0"
				request_length:                  462
				request_time:                    0.050
				status:                          200
				timestamp:                       "2023-03-18T15:00:00Z"
				upstream_addr:                   "10.0.50.80:9000"
				upstream_response_length:        19437
				upstream_response_time:          0.049
				upstream_status:                 200
			}
		},
	]
}
