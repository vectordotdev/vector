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

	#Function: {
		arguments: [...#Argument]
		internal_failure_reasons: [...string]
		return: [#Type, ...#Type]
		category:    #FunctionCategory
		description: string
		examples?: [#FunctionExample, ...#FunctionExample]
		name: string
	}

	#FunctionCategory: "Array" | "Check" | "Coerce" | "Decode" | "Encode" | "Enumerate" | "Event" | "Hash" | "IP" | "Map" | "Number" | "Parse" | "Random" | "String" | "Test" | "Timestamp"

	#FunctionExample: {
		title: string
		configuration?: [string]: string
		input:   #Event
		source:  string
		raises?: string

		if raises == _|_ {
			output: #Event
		}
	}

	functions: [Name=string]: #Function & {
		name: Name
	}
}
