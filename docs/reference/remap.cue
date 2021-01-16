package metadata

remap: {
	#RemapParameterTypes: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "path" | "string" | "regex" | "timestamp"

	#RemapReturnTypes: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "string" | "timestamp"

	{
		description: #"""
			**Vector Remap Language** (VRL) is a lean, single-purpose language for transforming observability data
			(logs and metrics) in Vector. It features a simple [syntax](#syntax) and a rich set of built-in
			[functions](#functions) tailored specifically to observability use cases. VRL is built on the following
			two principles:

			1. **Performance** — VRL is implemented in the very fast and efficient [Rust](\(urls.rust)) language and
			   VRL scripts are compiled into Rust code when Vector is started. This means that you can use VRL to
			   transform observability with a minimal per-event performance penalty vis-à-vis pure Rust. In addition,
			   ergonomic features such as compile-time correctness checks and the lack of language constructs like
			   loops make it difficult to write scripts that are slow or buggy or require optimization.
			2. **Safety** - VRL is a safe language in several senses: VRL scripts have access only to the event data
			   that they handle and not, for example, to the Internet or the host; VRL provides the same strong memory
			   safety guarantees as Rust; and, as mentioned above, compile-time correctness checks prevent VRL
			   scripts from behaving in unexpected or sub-optimal ways. These factors distinguish VRL from other
			   available event data transformation languages and runtimes.

			For a more in-depth picture, see the [announcement blog post](\(urls.vrl_announcement)) for more details.
			"""#

		functions: [Name=string]: {
			#Argument: {
				name:        string
				description: string
				required:    bool
				multiple:    bool | *false
				default?:    bool | string | int
				type: [#RemapParameterTypes, ...#RemapParameterTypes]
				enum?: [_description=string]: string
			}
			#RemapExample: {
				title: string
				configuration?: [string]: string
				input:   #Event
				source:  string
				raises?: string

				if raises == _|_ {
					output: #Event
				}
			}

			arguments: [...#Argument] // Allow for empty list
			internal_failure_reasons: [...string]
			return: [#RemapReturnTypes, ...#RemapReturnTypes]
			category:    "Array" | "Check" | "Coerce" | "Decode" | "Encode" | "Enumerate" | "Event" | "Hash" | "IP" | "Log" | "Map" | "Number" | "Parse" | "Random" | "String" | "System" | "Test" | "Timestamp"
			description: string
			examples?: [#RemapExample, ...#RemapExample]
			name: Name
		}
	}

	// VRL type system
	types: [TypeName=string]: {
		#Use: "parameter" | "return"

		description: string
		use: [#Use, ...#Use]
		examples?: [string, ...string]
	}

	types: {
		"array": {
			description: """
				A list of values. Items in a VRL array can be of any type, including other arrays
				and `null` (which is a value in VRL). You can access values insude arrays via
				index (beginning with 0). For the array `primary = [\"magenta\", \"yellow\",
				\"cyan\"]`, for example, `primary[0]` yields `\"magenta\"`.

				You can also assign values to arrays by index:

				```js
				stooges = [\"Larry\", \"Moe\"]
				stooges[2] = \"Curly\"
				```

				You can even assign values to arbitrary indices in arrays; indices that need to be
				created are back-filled as `null`. For example, if the `hobbies` field doesn't
				exist, the expression `.hobbies[2] = \"Pogs\"` sets the `hobbies` field to the array
				`[null, null, \"Pogs\"]`.

				And because all expressions in VRL are technically values, if you put expressions
				in arrays, the expressions are evaluated.

				```js
				.strange = [false || \"booper\", 1 + 1, 1 == 2]
				```

				In this case, `.strange` would evalue to `[\"booper\", 2, false]`.
				"""
			use: ["parameter", "return"]
			examples: [
				"[200, 201, 202, 204]",
				#"["error", "warn", "emerg"]"#,
				"[[1, 2, 3], [4, 5, 6]]",
				#"[true, 10, {"foo": "bar"}, [10], 47.5]"#,
				#"[1 + 1, 2 == 5, "hello" + " " + "world"]"#,
			]
		}
		"boolean": {
			description: "`true` or `false`."
			use: ["parameter", "return"]
			examples: [
				"true",
				"false",
			]
		}
		"float": {
			description: """
				A 64-bit floating-point number. Both positive and negative floats are supported.
				"""
			use: ["parameter", "return"]
			examples: [
				"0.0",
				"47.5",
				"-459.67",
			]
		}
		"map": {
			description: """
				A key-value map in which keys are strings and values can be of any VRL type,
				including other maps.

				As with arrays, you can use expressions to provide the value for a key:

				```js
				.user = { \"username\": exists(.username) || \"none\" }
				```

				Nested maps are supported:

				```js
				.user = {
					\"username\": \"thrillho\",
					\"metadata\": {
						\"level\": \"warrior\"
					}
				}
				```
				"""
			use: ["parameter", "return"]
			examples: [
				#"{"code": 200, "error_type": "insufficient_resources"}"#,
				#"""
					{
					  "user": {
					    "id": "tonydanza1337",
						"pricing_plan": "elite",
						"boss": true
					  }
					}
					"""#,
			]
		}
		"integer": {
			description: "A 64-bit integer. Both positive and negative integers are supported."
			use: ["parameter", "return"]
			examples: [
				"0",
				"1337",
				"-25",
			]
		}
		"null": {
			description: """
				No value. In VRL, you can assign `null` to fields and variables:

				```js
				.hostname = null
				code = null
				```

				`null` is the return value of expressions that don't return any other value, such
				as any invocation of the [`assert`](#assert) function.

				`null` is convertable to other types using `to_*` functions like `to_string`:

				Function | Output
				:----|:----------
				`to_string` | `""`
				`to_int` | `0`
				`to_bool` | `false`
				`to_float` | `0`
				"""
			use: ["parameter", "return"]
			examples: [
				"null",
			]
		}
		"regex": {
			description: """
				A **reg**ular **ex**pression. In VRL, regular expressions are delimited by `/` and
				use [Rust regex syntax](\(urls.rust_regex_syntax)). Here's an example usage of a
				regular expression:

				```js
				text_matches = match("happy", /(happy|sad)/)
				```

				### Flags

				VRL regular expressions allow three flags:

				Flag | Description
				:----|:-----------
				`x` | Ignore whitespace
				`i` | Case insensitive
				`m` | Multi-line mode

				Regex flags can be combined, as in `/pattern/xmi`, `/pattern/im`, etc.

				To learn more about regular expressions in Rust—and by extension in VRL—we strongly
				recommend the in-browser [Rustexp expression editor and
				tester](\(urls.regex_tester)).

				### Limitations

				There are a few things that you can't do with regexes in VRL:

				* You can't assign a regex to an object path. Thus, `.pattern = /foo|bar/i` is not
					allowed.
				* Expressions can't return regexes. Thus you can't, for example, dynamically create
					regular expressions.
				"""
			use: ["parameter"]
			examples: [
				#"/^http\://[a-zA-Z0-9\-\.]+\.[a-zA-Z]{2,3}(/\S*)?$/"#,
				#"""
					has_foo_or_bar = match("does contain foo", /(foo|bar)/)
					"""#,
			]
		}
		"string": {
			description: """
				A sequence of characters. A few things to note about VRL strings:

				* VRL converts strings in scripts to [UTF-8](\(urls.utf8)) and replaces any invalid
					sequences with `U+FFFD REPLACEMENT CHARACTER` (�).
				* Strings can be escaped using a forward slash (`\\`), as in `\"The song is called
					\"My name is Jonas\"\"`.
				* Multi-line strings *are* allowed and don't require any special syntax. See the
					multi-line example below.

				You can concatenate strings using plus (`+`). Here's an example:

				```js
				name = \"Vector Vic\"
				.message = name + \" is a pretty great mascot\" + \" (though we're a bit biased)\"
				```
				"""
			use: ["parameter", "return"]
			examples: [
				"\"I am a teapot\"",
				#"""
					"I am split \
					across multiple lines"
					"""#,
				#"This is not escaped, \"but this is\""#,
			]
		}
		"timestamp": {
			description: """
				A string formatted as a timestamp. Timestamp specifiers can be either:

				1. One of the built-in-formats listed in the [Timestamp Formats](#timestamp-formats)
					table below, or
				2. Any valid sequence of [time format specifiers](\(urls.chrono_time_formats)) from
					Rust's `chrono` library.

				### Timestamp Formats

				The examples in this table are for 54 seconds after 2:37 am on December 1st, 2020 in
				Pacific Standard Time.

				Format | Description | Example
				:------|:------------|:-------
				`%F %T` | `YYYY-MM-DD HH:MM:SS` | `2020-12-01 02:37:54`
				`%v %T` | `DD-Mmm-YYYY HH:MM:SS` | `01-Dec-2020 02:37:54`
				`%FT%T` | [ISO 8601](\(urls.iso_8601))\\[RFC 3339](\(urls.rfc_3339)) format without time zone | `2020-12-01T02:37:54`
				`%a, %d %b %Y %T` | [RFC 822](\(urls.rfc_822))/[2822](\(urls.rfc_2822)) without time zone | `Tue, 01 Dec 2020 02:37:54`
				`%a %d %b %T %Y` | [`date`](\(urls.date)) command output without time zone | `Tue 01 Dec 02:37:54 2020`
				`%a %b %e %T %Y` | [ctime](\(urls.ctime)) format | `Tue Dec  1 02:37:54 2020`
				`%s` | [UNIX](\(urls.unix_timestamp)) timestamp | `1606790274`
				`%FT%TZ` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC | `2020-12-01T09:37:54Z`
				`%+` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC with time zone | `2020-12-01T02:37:54-07:00`
				`%a %d %b %T %Z %Y` | [`date`](\(urls.date)) command output with time zone | `Tue 01 Dec 02:37:54 PST 2020`
				`%a %d %b %T %z %Y`| [`date`](\(urls.date)) command output with numeric time zone | `Tue 01 Dec 02:37:54 -0700 2020`
				`%a %d %b %T %#z %Y` | [`date`](\(urls.date)) command output with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`
				"""
			use: ["parameter", "return"]
			examples: [
				"%a %d %b %T %Y",
				"%FT%TZ",
			]
		}
	}

	// VRL syntax
	#Operators: [_category=string]: [_op=string]: string

	syntax: [RuleName=string]: {
		#InNOut: {
			in:  string
			out: #RemapReturnTypes
		}

		name:        RuleName
		href:        string // Ensures that we don't end up with clashing anchors
		description: string
		examples?: [string, ...string]
		in_n_out?: [#InNOut, ...#InNOut]
		operators?: #Operators
		warnings?: [string, ...string]
	}

	syntax: {
		"Paths": {
			href: "paths"

			description: """
				In VRL, a dot (`.`) holds state across the script. At the beginning of the script,
				it represents the object arriving into the transform; that object can be a log or a
				metric. To give an example, imagine you're writing a VRL script to handle logs in
				[JSON](\(urls.json)) format. Here's an example event:

				```json
				{"status_code":200,"username":"booper1234","message":"Successful transaction"}
				```

				In this case, the event object, represented by the dot, has three fields:
				`.status_code`, `.username`, and `.message`. You can assign new values to the
				existing fields (`.message = "something different"`), add new fields (`.new_field =
				"new value"`), delete fields (`del(.username)`), store those values in variables
				(`code = .status_code`), and more.

				### Nested values

				The dot syntax can represent nested fields, for example `.transaction.id` or
				`.geo.latitude`. If you assign values to nested fields whose parent fields don't
				exist, the parent fields are created. Take this expression as an example:

				```
				.user.info.hobbies = ["cooking", "Pogs"]
				```

				If the `user` field doesn't exist, it is created; if `.user` exists but `.user.info`
				doesn't, `.user.info` is created; and so on.

				### Path coalescing

				Path *coalescing* in VRL enables you to express "or" logic inside of paths. This
				expression sets `user.first_name` to `"Feldman"` *if* that field exists; if not, the
				`user.last_name` field is set to `"Feldman"` instead.

				```
				.user.(first_name | last_name) = "Feldman"
				```

				### Quoted paths

				In the examples above, all paths have used literals à la `.foo.bar`. But path
				segments can also be quoted, as in this example:

				```js
				user.preferences."favorite color" = "chartreuse"
				```

				Quoted paths are particularly useful when keys need to contain whitespace.

				### Indexing

				Values inside VRL arrays can be accessed via index (beginning with 0). For the array
				`primary = ["magenta", "yellow", "cyan"]`, `primary[0]` would yield `"magenta"`.

				You can also assign values to arrays by index:

				```js
				stooges = ["Larry", "Moe"]
				stooges[2] = "Curly"
				```

				You can even assign values to arbitrary indices in arrays; any indices that need to
				be created are back-filled as `null`. For example, if the `hobbies` field doesn't
				exist, the expression `.hobbies[2] = "Pogs"` sets `hobbies` to `[null, null,
				"Pogs"]`.

				### Combined

				All of the path methods above can be combined in any way. Here's an example of a
				complex path:

				```js
				.transaction.(metadata | info).orders[0] = "a1b2c3d4e5f6"
				```

				This sets the first element of `.transaction.metadata.orders[0]` to `"a1b2c3d4e5f6"`
				*if* `.transaction.metadata` exists; if not, it sets `.transaction.info.orders[0]`
				to `"a1b2c3d4e5f6"` instead. If neither exists,
				"""
			examples: [
				".",
				".status_code",
				#".message.event."time of occurrence""#,
				".transaction.id",
				".user.hobbies[0].description",
				".event.(time | timestamp).format",
				#""#,
			]
		}

		"Expressions": {
			href: "expressions"

			description: """
				*All* expressions in VRL resolve to a value. Expressions come in four kinds, listed
				below, each of which resolves in a different way:

				Expression type | Resolves to
				:---------------|:-----------
				Assignment | The assigned value
				Control flow statements | The value returned by the chosen expression
				Boolean expressions | The returned Boolean value
				Blocks | The value returned by the last expression in the block
				"""

			in_n_out: [
				{in: #".request_id = "a1b2c3d4e5f6""#, out:                      "string"},
				{in: #"if (starts_with("v1", .version)) { .version = 1 }"#, out: "integer"},
				{in: #"contains("emergency", "this is an emergency")"#, out:     "boolean"},
				{
					in: """
						is_success = { code = .status_code; del(.status_code); code == 200 }
						"""
					out: "boolean"
				},
			]
		}

		"Lines": {
			href: "lines"

			description: #"""
				VRL expressions can be split across multiple lines using a backslash (`\`):

				```js
				del(.one, .two, .three, .four \
					.five, .six)
				```

				This statement is semantically identical to `del(.one, .two, .three, .four, .five,
				.six)`.

				Conversely, multiple expressions can be collapsed into a single line using a
				semicolon (`;`) as the separator:

				```js
				success_code = 200; .success = .success_code == success_code; del(.success_code)
				```

				You can also use line collapsing via semicolon in [control flow
				statements](#control-flow).
				"""#
		}

		"Functions": {
			href: "functions"

			description: """
				In VRL, functions can take inputs (or no input) and return either a value or, in the
				case of some functions, an error.
				"""

			examples: [
				"parse_json(.message)",
				"assert(.status_code == 500)",
				#"ip_subnet(.address, "255.255.255.0")"#,
				".request_id = uuid_v4()",
			]
		}

		"Control flow": {
			href: "control-flow"

			description: """
				VRL supports control flow operations using `if`, `else if`, and `else`. These can
				be called on any expression that returns a Boolean. Here's a generic example of the
				syntax:

				```js
				if (condition) {
					...
				} else if (other_condition) {
					...
				} else {
					...
				}
				```

				Any number of expressions can be combined inside of a block if they're separated by
				a semicolon (`;`), provided that the last expression resolves to a Boolean:

				```js
				.password = "sesame"

				if (keyword = "sesame"; .password == keyword) {
					.entry = true
					del(.password)
				}
				```
				"""

			examples: [
				"""
					if (match("this does contain foo", /(foo|bar)/)) {
						.contains_foo = true
					} else {
						.does_not_contain_foo = true
					}
					""",
			]
		}

		"Error handling": {
			href: "error-handling"

			description: #"""
				In VRL, functions that throw errors are known as *fallible* and functions that don't
				are known as *infallible*. When functions are fallible—and most functions are—you
				need to handle the errors that they throw.

				### Error coalescing

				In VRL, you can *coalesce* errors using the `??` operator. Coalescence means that if
				a function throws an error, you can defer to another function using `??`. Here's an
				example:

				```js
				.code = to_int(del(.status)) ?? 200
				```

				Here, if the `status` field doesn't exist, the `to_int` call fails; with the `??` in
				place, the error is ignored and the next function is called. These coalescence
				chains can be of any length. Here's a chain with three expressions:

				```js
				.status = to_string(del(.status)) ?? parse_syslog_level(del(.level)) ?? "ok"
				```
				"""#
		}

		"Assignment": {
			href: "assignment"

			description: #"""
				You can assign values to object fields or [variables](#variables) using a single
				equals sign (`=`). Some examples:

				* `.is_success = .code > 200 && .code <= 299`
				* `.request.id = uuid_v4()`
				* `is_severe = to_syslog_level(.severity) == "severity"

				In VRL, `=` represents assignment while `==` is a [comparison operator](#operators),
				as in many programming languages.

				If you assign a value to an object field that doesn't already exist, the field is
				created; if the field does exist, the value is re-assigned.

				### Conditional assignment

				In VRL, *conditional* assignment is assignment to a field or variable only if the
				right-hand-side expression succeeds. This form of assignment uses the `??=`
				operator (a combination of `=` for assignment and `??` for
				[coalescence](#coalescence)). Here's an example:

				```js
				. ??= parse_json(.message)
				```

				In this case, the root event field is assigned the parsed JSON value of the
				`message` *only if JSON parsing succeeds*; if it fails, the assignment doesn't
				occur.
				"""#

			examples: [
				".request_id = uuid_v4()",
				"average = to_float(.total / .number)",
				".partition_id = .status_code",
				".is_server_error = .status_code == 500",
				". ??= parse_json(.message)",
				".timestamp ??= to_unix_timestamp(.timestamp)",
			]
		}

		"Variables": {
			href: "variables"

			description: """
				You can assign values to variables in VRL. You can use any bare word for a variable
				in VRL, with the exception of VLR's [reserved terms](#reserved-terms). Here's an
				example usage of a variable:

				```js
				log_level = "critical"
				.log_level = log_level
				```

				### Assignment using expressions

				Because all VRL expressions return a value (by definition), you can assign the
				result of expressions to variables as well:

				```js
				is_success = .status_code == 200
				has_buzzword = contains(.message, "serverless")
				```

				### Reserved terms

				The following terms are reserved in VRL and thus can't be used as variable names:
				`if`, `else`, `for`, `while`, `loop`, `abort`, `break`, `continue`, `return`, `as`,
				`type`, `let`, `until`, `then`, `impl`, `in`, `self`, `this`, `use`, `std`, `null`,
				`boolean`.

				Note that of these terms are not currently used in the language, but may be in the
				future. The goal of pre-reserving these terms is to provide as much backward
				compatibility as possible as VRL changes over time.
				"""

			examples: [
				"status_code = .code",
				#"is_critical = .log_level == "critical""#,
				#"creepy_greeting = "Hello, Clarice""#,
				#"""
					is_url = match(.url, /^http(s):\/\/[a-zA-Z0-9\-\.]+\.[a-zA-Z]{2,3}(\/\S*)?$/)
					.has_proper_format = is_url
					del(.url)
					"""#,
			]
		}

		"Blocks": {
			href: "blocks"

			description: """
				VRL supports organizing expressions into blocks using curly braces. Everything
				inside of a block is evaluated as a single expression. In this example, the value
				assigned to the variable `success` is `true` if the value of the `status_code`
				field is `201`:

				```js
				.very_important_event = {
					fail_code = .status_code >= 500
					paying_customer = .user.plan == \"enterprise\"

					fail_code && paying_customer
				}
				```

				You can also collapse blocks into a single line by separating the expressions with a
				semicolon (`;`), as in this block:

				```js
				not_important = { success_code = .status_code == 200; debug_msg = .level == \"debug\"; success_code && debug_msg }
				```
				"""

			examples: [
				#"""
					very_important = {
						fail_code = .status_code >= 500
						del(.status_code)
						paying_customer = .user.plan == "enterprise"
						del(.user)

						fail_code && paying_customer
					}

					if very_important {
						.important = true
					}
					"""#,
			]
		}

		"Comments": {
			href: "comments"

			description: #"""
				VRL scripts allow for comments. Anything after `#` in a given line is considered a
				comment, which means that entire lines can be commented out or only the latter part
				of a line:

				```js
				# An old standby placeholder:
				.lorem = "ipsem"

				.stooges = ["Larry", "Moe"] # No Curly this time, sorry
				```

				As VRL scripts are pre-compiled, the presence of comments has no effect on
				processing speed, so feel free to use comments as liberally as you need to.
				"""#

			examples: [
				"""
					// This is not processed
					""",
				"""
					.this_is_processed = 123 // But this is ignored
					""",
			]
		}

		"Operators": {
			href: "operators"

			description: """
				VRL offers a standard set of operators, listed in the table below, that should be
				familiar from many other programming languages.

				VRL supports the standard [order of operations](\(urls.order_of_ops)). Thus,
				`(2 * 2) + 8` makes `12`, `10 / (2 + 3)` makes `2`, `true && (false || true)` makes
				`true`, and so on.
				"""

			examples: [
				"exists(.request_id) && !exists(.username)",
				".status_code == 200",
				#".user.plan != "enterprise" && .user.role == "admin""#,
			]

			operators: {
				"Boolean": {
					"&&": "And"
					"||": "Or"
					"!":  "Not"
				}
				"Equality": {
					"==": "Equals"
					"!=": "Not equals"
				}
				"Comparison": {
					">":  "Greater than"
					"<":  "Less than"
					">=": "Greater than or equal to"
					"<=": "Less than or equal to"
				}
				"Arithmetic": {
					"+": "Plus"
					"-": "Minus"
					"/": "Divide by"
					"*": "Multiply by"
					"%": "Modulo"
				}
				"Error handling and assignment": {
					"??=": "[Conditional assignment](#conditional-assignment)"
					"??":  "[Error coalescence](#error-coalescence)"
				}
			}
		}
	}
}
