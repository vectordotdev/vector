package metadata

remap: {
	#RemapParameterTypes: "path" | "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "regex" | "any"

	#RemapReturnTypes: "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "null"

	{
		description: """
			The Timber Remap Language (TRL) is a purpose-driven, Rust-native data
			mapping language that enables Vector users to easily map and reshape data
			without sacrificing performance or safety. It's a middle ground between
			stringing together many fundamental [transforms](\(urls.vector_transforms))
			and a full blown runtime like Lua. Principles of TRL include:

			1. **Performance** - Beyond extremely fast execution, TRL is designed to
			   prevent operators from writing slow scripts.
			2. **Safety** - TRL is Rust-native and performs compile-time checks on
			   boot that ensure safety. In addition, TRL is designed for
			   collaboration. It is intentionally simple, avoiding footguns introduced
			   through complexity.
			3. **Easy** - A TRL script is obvious at first glance. It is designed to
			   have little, if any, learning curve.

			TRL is designed and maintained by Timber and built specific for processing
			data within Vector.
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
			category:    "coerce" | "object" | "parse" | "text" | "hash" | "event" | "networking"
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
}
