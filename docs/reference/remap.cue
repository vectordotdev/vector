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

	// TRL types
	types: [TypeName=string]: {
		#Use: "parameter" | "return"

		description: string
		use: [#Use, ...#Use]
		examples: [string, ...string]
	}

	types: {
		"any": {
			description: ""
			use: ["parameter"]
		}
		"array": {
			description: ""
			use: ["parameter", "return"]
		}
		"boolean": {
			description: ""
			use: ["parameter", "return"]
		}
		"float": {
			description: ""
			use: ["parameter", "return"]
		}
		"map": {
			description: ""
			use: ["parameter", "return"]
		}
		"integer": {
			description: ""
			use: ["parameter", "return"]
		}
		"null": {
			description: ""
			use: ["return"]
		}
		"path": {
			description: ""
			use: ["parameter"]
		}
		"regex": {
			description: ""
			use: ["parameter"]
		}
		"string": {
			description: ""
			use: ["parameter", "return"]
		}
		"timestamp": {
			description: ""
			use: ["parameter", "return"]
		}
	}

	// TRL syntax
	syntax: [RuleName=string]: {
		name:        RuleName
		href:        string // Ensures that we don't end up with clashing anchors
		description: string
		examples: [string, ...string]
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
			description: """
				In TRL, functions act just like they do in standard programming languages. They can take
				inputs (or not) and either return values or make an [assertion](#assert) (if the
				assertion fails, the script stops executing and Vector logs an error).
				"""

			examples: [
				"parse_json(.message)",
				"assert(.status_code == 500)",
				#"ip_subnet(.address, "255.255.255.0")"#,
			]
		}
	}
}
