package metadata

remap: {
	#RemapParameterTypes: "path" | "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "regex" | "any"

	#RemapReturnTypes: "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "null"

	{
		description: """
			The Timber Remap Language (TRL) is a single-purpose, [Rust](\(urls.rust))-native data
			mapping language that enables you to easily map and reshape data without sacrificing
			performance or safety. It occupies a comfortable middle ground between stringing
			together fundamental [transforms](\(urls.vector_transforms)) and using a full-blown
			runtime like [Lua](\(urls.lua)). Guiding principles behind TRL include:

			1. **Performance** - Beyond extremely fast execution, TRL is designed to prevent
			   Vector operators from writing slow scripts.
			2. **Safety** - TRL is Rust native and performs compile-time checks at boot time to
			   ensure safety. In addition, TRL's simplicity and lack of complex \"footguns\" are
			   ideal for collaboration.
			3. **Easy** - A TRL script's intentions are clear even at first glance because the
			   language is designed to have a very gentle learning curve.

			TRL is designed and maintained by [Timber](\(urls.timber)) and built specifically for
			processing data within Vector.
			"""

		errors: [Name=string]: {
			description: string
			name:        Name
		}

		functions: [Name=string]: {
			#Argument: {
				name:        string
				description: string
				required:    bool
				multiple:    bool | *false
				default?:    bool | string | int
				type: [#RemapParameterTypes, ...#RemapParameterTypes]
			}
			#RemapExample: {
				title: string
				configuration?: [string]: string
				input:  #Fields
				source: string
				output: #Fields
			}

			arguments: [...#Argument] // Allow for empty list
			return: [#RemapReturnTypes, ...#RemapReturnTypes]
			category:    "coerce" | "numeric" | "object" | "parse" | "text" | "hash" | "event" | "networking"
			description: string
			examples: [#RemapExample, ...#RemapExample]
			name: Name
		}
	}

	errors: {
		ArgumentError: {
			description: "Raised when the provided input is not a supported type."
		}
		ParseError: {
			description: "Raised when the provided input cannot be parsed."
		}
	}

	// TRL type system
	types: [TypeName=string]: {
		#Use: "parameter" | "return"

		description: string
		use: [#Use, ...#Use]
	}

	types: {
		"any": {
			description: "A stand-in for any type."
			use: ["parameter"]
		}
		"array": {
			description: "A list of items."
			use: ["parameter", "return"]
		}
		"boolean": {
			description: "`true` or `false`."
			use: ["parameter", "return"]
		}
		"float": {
			description: "A 64-bit floating-point number."
			use: ["parameter", "return"]
		}
		"map": {
			description: """
				A key-value map in which keys are strings and values can be of any TRL type,
				including maps.
				"""
			use: ["parameter", "return"]
		}
		"integer": {
			description: "A 64-bit integer."
			use: ["parameter", "return"]
		}
		"null": {
			description: "No value."
			use: ["return"]
		}
		"path": {
			description: "An event field."
			use: ["parameter"]
		}
		"regex": {
			description: "A regular expression."
			use: ["parameter"]
		}
		"string": {
			description: """
				A sequence of characters. Remap converts strings in scripts to [UTF-8](\(urls.utf8))
				and replaces any invalid sequences with `U+FFFD REPLACEMENT CHARACTER` (�).
				"""
			use: ["parameter", "return"]
		}
		"timestamp": {
			description: "A string formatted as a timestamp."
			use: ["parameter", "return"]
		}
	}

	// TRL syntax
	#Operators: [_category=string]: [_op=string]: string

	syntax: [RuleName=string]: {
		name:        RuleName
		href:        string // Ensures that we don't end up with clashing anchors
		description: string
		examples: [string, ...string]
		operators?: #Operators
	}

	syntax: {
		"Dot notation": {
			href: "dot-notation"

			description: """
				In TRL, a dot (`.`) holds state across the script. At the beginning of the script, it
				represents the event arriving into the transform. Take this JSON event data as an
				example:

				```json
				{"status_code":200,"username":"booper1234","message":"Successful transaction"}
				```

				In this case, the dot has three fields: `.status_code`, `.username`, and `.message`. You
				can then assign new values to the existing fields (`.message = "something different"`),
				add new fields (`.new_field = "new value"`), and much more.

				The dot can also represent nested values, like `.transaction.id` or `.geo.latitude`.
				"""
			examples: [
				".status_code",
				".message",
				".username",
				".transaction.id",
				".geo.latitude",
			]
		}

		"Functions": {
			href: "functions"

			description: """
				In TRL, functions act just like they do in standard programming languages. They can
				take inputs (or no input) and either return values or make an
				[assertion](#assertion) using the [`assert`](#assert) function.

				TRL types that can serve as inputs to functions are listed in [Parameter
				types](#parameter-types); types that functions can output are listed in [Return
				types](#return-types).
				"""

			examples: [
				"parse_json(.message)",
				"assert(.status_code == 500)",
				#"ip_subnet(.address, "255.255.255.0")"#,
			]
		}

		"Assignment": {
			href: "assignment"

			description: """
				You can assign values to fields using a single equals sign (`=`). If the field already
				exists, its value is re-assigned; it the field doesn't already exist, it's created and
				assigned the value.
				"""

			examples: [
				".request_id = uuidv4()",
				".average = .total / .number",
				".partition_id = .status_code",
			]
		}

		"Assertion": {
			href: "assertion"

			description: """
				In TRL, you can assert that something is true using [`assert`](#assert). If an
				assertion succeeds, Vector does nothing and continues with the script; if it fails,
				the script stops executing and an error is logged. `assert` can take any Boolean as
				input, including functions that return Booleans, like [`exist`](#exist).
				"""

			examples: [
				#"assert(.level == "warn")"#,
				"assert(exists(.user.username))",
				"assert(10 == 10 && 1.0 == 1.0)",
				#"assert(ip_cidr_contains(.address, "192.168.0.0/16"))"#,
				#"assert(contains("Vector rocks", "vector", case_sensitive = false))"#,
			]
		}

		"Operators": {
			href: "operators"

			description: """
				TRL offers a standard set of operators that should be familiar from many other
				programming languages.
				"""

			examples: [
				"exists(.request_id) && !exists(.username)",
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
			}
		}
	}
}
