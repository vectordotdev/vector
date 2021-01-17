package metadata

remap2: functions: {
	#Argument: {
		name:        string
		description: string
		required:    bool
		multiple:    bool | *false
		default?:    bool | string | int
		type: [remap2.#Type, ...remap2.#Type]
		enum?: #Enum
	}

	#Function: {
		arguments: [...#Argument]
		internal_failure_reasons: [...string]
		return: [remap2.#Type, ...remap2.#Type]
		category:    #FunctionCategory
		description: string
		examples?: [remap2.#Example, ...remap2.#Example]
		name: string
	}

	#FunctionCategory: "Array" | "Check" | "Coerce" | "Decode" | "Encode" | "Enumerate" | "Event" | "Hash" | "IP" | "Map" | "Number" | "Parse" | "Random" | "String" | "Test" | "Timestamp"

	{[Name=string]: #Function & {
		name: Name
	}}
}
