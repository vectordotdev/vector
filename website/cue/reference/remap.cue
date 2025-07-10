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
		output?: #Event | [#Event, ...#Event]
		raises?: _

		notes?: [string, ...string]
		warnings?: [string, ...string]

		// whether to skip in doc tests
		skip_test?: bool
	}

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "object" | "null" | "path" | "string" | "regex" | "timestamp"

	concepts: _
	errors:   _
	examples: [#Example, ...#Example]
	expressions: _
	functions:   _
	function_categories: [string, ...string]
	how_it_works: #HowItWorks
	literals:     _
	principles:   _
	syntax:       _
	features:     _
}

remap: #Remap & {
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
				"Values of duplicate keys are grouped into an array.",
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
				message_parts = split(.message, ", ", limit: 2)
				structured = parse_key_value(message_parts[1], key_value_delimiter: ":", field_delimiter: ",") ?? {}
				.message = message_parts[0]
				. = merge(., structured)
				"""#
			output: log: {
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
				.tags.environment = get_env_var!("ENV") # add
				.tags.hostname = del(.tags.host) # rename
				del(.tags.email)
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
			title: "Emitting multiple logs from JSON"
			input: log: message: #"[{"message": "first_log"}, {"message": "second_log"}]"#
			source: """
				. = parse_json!(.message) # sets `.` to an array of objects
				"""
			output: [
				{log: {message: "first_log"}},
				{log: {message: "second_log"}},
			]
			notes: [
				"Setting `.` to an array will emit one event per element",
			]
		},
		{
			title: "Emitting multiple non-object logs from JSON"
			input: log: message: #"[5, true, "hello"]"#
			source: """
				. = parse_json!(.message) # sets `.` to an array
				"""
			output: [
				{log: {message: 5}},
				{log: {message: true}},
				{log: {message: "hello"}},
			]
			notes: [
				"Setting `.` to an array will emit one event per element. Any non-object elements will be set to the `message` key of the output event.",
			]
			skip_test: true
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
				  │        this expression resolves to the exact type integer
				  │        but the parameter "value" expects the exact type string
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
				  = try your code in the VRL REPL, learn more at https://vrl.dev/examples
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
				  │ │            this expression is fallible because at least one argument's type cannot be verified to be valid
				  │ │            update the expression to be infallible by adding a `!`: `parse_key_value!(.message)`
				  │ │            `.message` argument type is `any` and this function expected a parameter `value` of type `string`
				  │ or change this to an infallible assignment:
				  │ structured, err = parse_key_value(.message)
				  │
				  = see documentation about error handling at https://errors.vrl.dev/#handling
				  = see functions characteristics documentation at https://vrl.dev/expressions/#function-call-characteristics
				  = learn more about error code 103 at https://errors.vrl.dev/103
				  = see language documentation at https://vrl.dev
				  = try your code in the VRL REPL, learn more at https://vrl.dev/examples
				"""
		},
	]
}
