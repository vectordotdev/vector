package metadata

#Remap: {
	#Characteristic: {
		anchor:      name
		enum?:       #Enum
		name:        string
		title:       string
		description: string
	}

	#Characteristics: [Name=string]: #Characteristic & {
		name: Name
	}

	#Example: {
		title:   string
		input?:  #Event
		source:  string
		diff?:   string
		return?: _
		output?: #Event
		raises?: _

		notes?: [string, ...string]
		warnings?: [string, ...string]
	}

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "object" | "null" | "path" | "string" | "regex" | "timestamp"

	concepts:    _
	description: string
	errors:      _
	examples: [#Example, ...#Example]
	expressions:  _
	features:     _
	functions:    _
	how_it_works: #HowItWorks
	literals:     _
	principles:   _
	syntax:       _
}

remap: #Remap & {
	description: #"""
		**Vector Remap Language** (VRL) is an expression-oriented language designed for transforming observability data
		(logs and metrics) in a [safe](\#(urls.vrl_safety)) and [performant](\#(urls.vrl_performance)) manner. It
		features a simple [syntax](\#(urls.vrl_expressions)) and a rich set of built-in
		[functions](\#(urls.vrl_functions)) tailored specifically to observability use cases.

		You can use VRL in Vector via the [`remap` transform](\#(urls.vector_remap_transform)), and for a more in-depth
		picture, see the [announcement blog post](\#(urls.vrl_announcement)).
		"""#

	examples: [
		{
			title: "Parse Syslog logs"
			input: log: message: "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
			source: """
				. |= parse_syslog!(.message)
				"""
			output: log: {
				appname:   "su"
				facility:  "ntp"
				hostname:  "vector-user.biz"
				message:   "Something went wrong"
				msgid:     "ID389"
				procid:    2666
				severity:  "info"
				timestamp: "2020-12-22T15:22:31.111Z"
				version:   1
			}
			notes: [
				"Attributes are coerced into their proper types, including `timestamp`.",
			]
		},
		{
			title: "Parse key/value (logfmt) logs"
			input: log: message: "@timestamp=\"Sun Jan 10 16:47:39 EST 2021\" level=info msg=\"Stopping all fetchers\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"
			source: """
				. = parse_key_value!(.message)
				"""
			output: log: {
				"@timestamp":     "Sun Jan 10 16:47:39 EST 2021"
				level:            "info"
				msg:              "Stopping all fetchers"
				"tag#production": "stopping_fetchers"
				id:               "ConsumerFetcherManager-1382721708341"
				module:           "kafka.consumer.ConsumerFetcherManager"
			}
			warnings: [
				"All attributes are strings and will require manual type coercing.",
			]
		},
		{
			title: "Parse custom logs"
			input: log: message: #"2021/01/20 06:39:15 +0000 [error] 17755#17755: *3569904 open() "/usr/share/nginx/html/test.php" failed (2: No such file or directory), client: xxx.xxx.xxx.xxx, server: localhost, request: "GET /test.php HTTP/1.1", host: "yyy.yyy.yyy.yyy""#
			source: #"""
				. |= parse_regex!(.message, r'^(?P<timestamp>\d+/\d+/\d+ \d+:\d+:\d+ \+\d+) \[(?P<severity>\w+)\] (?P<pid>\d+)#(?P<tid>\d+):(?: \*(?P<connid>\d+))? (?P<message>.*)$')

				# Coerce parsed fields
				.timestamp = parse_timestamp(.timestamp, "%Y/%m/%d %H:%M:%S %z") ?? now()
				.pid = to_int!(.pid)
				.tid = to_int!(.tid)

				# Extract structured data
				message_parts = split(.message, ", ", limit: 2) ?? []
				structured = parse_key_value(message_parts[1], key_value_delimiter: ":", field_delimiter: ",") ?? {}
				.message = message_parts[0]
				. = merge(., structured)
				"""#
			output: log: {
				"0":       "2021/01/20 06:39:15 +0000 [error] 17755#17755: *3569904 open() \"/usr/share/nginx/html/test.php\" failed (2: No such file or directory), client: xxx.xxx.xxx.xxx, server: localhost, request: \"GET /test.php HTTP/1.1\", host: \"yyy.yyy.yyy.yyy\""
				"1":       "2021/01/20 06:39:15 +0000"
				"2":       "error"
				"3":       "17755"
				"4":       "17755"
				"5":       "3569904"
				"6":       "open() \"/usr/share/nginx/html/test.php\" failed (2: No such file or directory), client: xxx.xxx.xxx.xxx, server: localhost, request: \"GET /test.php HTTP/1.1\", host: \"yyy.yyy.yyy.yyy\""
				timestamp: "2021-01-20T06:39:15Z"
				severity:  "error"
				pid:       17755
				tid:       17755
				connid:    "3569904"
				message:   #"open() "/usr/share/nginx/html/test.php" failed (2: No such file or directory)"#
				client:    "xxx.xxx.xxx.xxx"
				server:    "localhost"
				request:   "GET /test.php HTTP/1.1"
				host:      "yyy.yyy.yyy.yyy"
			}
		},
		{
			title: "Multiple parsing strategies"
			input: log: message: "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
			source: #"""
				structured =
				  parse_syslog(.message) ??
				  parse_common_log(.message) ??
				  parse_regex!(.message, r'^(?P<timestamp>\d+/\d+/\d+ \d+:\d+:\d+) \[(?P<severity>\w+)\] (?P<pid>\d+)#(?P<tid>\d+):(?: \*(?P<connid>\d+))? (?P<message>.*)$')
				. = merge(., structured)
				"""#
			output: log: {
				appname:   "su"
				facility:  "ntp"
				hostname:  "vector-user.biz"
				message:   "Something went wrong"
				msgid:     "ID389"
				procid:    2666
				severity:  "info"
				timestamp: "2020-12-22T15:22:31.111Z"
				version:   1
			}
		},
		{
			title: "Modify metric tags"
			input: metric: {
				kind: "incremental"
				name: "user_login_total"
				counter: {
					value: 102.0
				}
				tags: {
					host:        "my.host.com"
					instance_id: "abcd1234"
					email:       "vic@vector.dev"
				}
			}
			source: #"""
				.environment = get_env_var!("ENV") # add
				.hostname = del(.host) # rename
				del(.email)
				"""#
			output: metric: {
				kind: "incremental"
				name: "user_login_total"
				counter: {
					value: 102.0
				}
				tags: {
					environment: "production"
					hostname:    "my.host.com"
					instance_id: "abcd1234"
				}
			}
		},
		{
			title: "Invalid argument type"
			input: log: not_a_string: 1
			source: """
				upcase(42)
				"""
			raises: compiletime: """
				error[E110]: invalid argument type
				  ┌─ :1:8
				  │
				1 │ upcase(42)
				  │        ^^
				  │        │
				  │        this expression resolves to the exact type "integer"
				  │        but the parameter "value" expects the exact type "string"
				  │
				  = try: ensuring an appropriate type at runtime
				  =
				  =     42 = string!(42)
				  =     upcase(42)
				  =
				  = try: coercing to an appropriate type and specifying a default value as a fallback in case coercion fails
				  =
				  =     42 = to_string(42) ?? "default"
				  =     upcase(42)
				  =
				  = see documentation about error handling at https://errors.vrl.dev/#handling
				  = learn more about error code 110 at https://errors.vrl.dev/110
				  = see language documentation at https://vrl.dev
				"""
		},
		{
			title: "Unhandled fallible assignment"
			input: log: message: "key1=value1 key2=value2"
			source: """
				structured = parse_key_value(.message)
				"""
			raises: compiletime: """
				error[E103]: unhandled fallible assignment
				  ┌─ :1:14
				  │
				1 │ structured = parse_key_value(.message)
				  │ ------------ ^^^^^^^^^^^^^^^^^^^^^^^^^
				  │ │            │
				  │ │            this expression is fallible
				  │ │            update the expression to be infallible
				  │ or change this to an infallible assignment:
				  │ structured, err = parse_key_value(.message)
				  │
				  = see documentation about error handling at https://errors.vrl.dev/#handling
				  = learn more about error code 103 at https://errors.vrl.dev/103
				  = see language documentation at https://vrl.dev
				"""
		},
	]

	how_it_works: {
		event_data_model: {
			title: "Event Data Model"
			body:  """
				You can use the `remap` transform with both log and metric events. Log events in the `remap` transform
				correspond directly to Vector's [log schem](\(urls.vector_log)), which means that the transform has
				access to the whole event. With metric events, however, the remap transform only has access to the
				event's tags.
				"""
		}
	}
}
