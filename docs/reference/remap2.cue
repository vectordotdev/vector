package metadata

remap2: {
	#Argument: {
		name:        string
		description: string
		required:    bool
		multiple:    bool | *false
		default?:    bool | string | int
		type: [#Type, ...#Type]
		enum?: #Enum
	}

	#Construct: {
		title: string
		description: string
		types: [...#ConstructType]
	}

	#ConstructType: {
		title: string
		description :string
	}

	#Example: {
		title: string
		configuration?: [string]: string
		input:   #Event
		source:  string
		raises?: string

		if raises == _|_ {
			output: #Event
		}
	}

	#Function: {
		arguments: [...#Argument]
		internal_failure_reasons: [...string]
		return: [#Type, ...#Type]
		category:    #FunctionCategory
		description: string
		examples?: [#Example, ...#Example]
		name: string
	}

	#FunctionCategory: "Array" | "Check" | "Coerce" | "Decode" | "Encode" | "Enumerate" | "Event" | "Hash" | "IP" | "Map" | "Number" | "Parse" | "Random" | "String" | "Test" | "Timestamp"

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "path" | "string" | "regex" | "timestamp"

	functions: [Name=string]: #Function & {
		name: Name
	}

	syntax: [Name=string]: #Construct & {
		name: Name
	}
}
