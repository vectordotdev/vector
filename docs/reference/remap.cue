package metadata

remap: {
	#RemapParameterTypes: "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "regex" | "any"

	#RemapReturnTypes: "float" | "integer" | "string" | "timestamp" | "boolean" | "array" | "map" | "null"

	{
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
				default?:    bool | string
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
			category:    "coerce" | "parse" | "text" | "hash" | "event"
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
