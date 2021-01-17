package metadata

remap: functions: {
	#Argument: {
		name:        string
		description: string
		required:    bool
		multiple:    bool | *false
		default?:    bool | string | int
		type: [remap.#Type, ...remap.#Type]
		enum?: #Enum
	}

	#Function: {
		arguments: [...#Argument]
		internal_failure_reasons: [...string]
		return: [remap.#Type, ...remap.#Type]
		category:    #FunctionCategory
		description: string
		examples?: [remap.#Example, ...remap.#Example]
		name: string
	}

	#FunctionCategory: "Array" | "Check" | "Coerce" | "Decode" | "Encode" | "Enumerate" | "Event" | "Hash" | "IP" | "Log" | "Map" | "Number" | "Parse" | "Random" | "String" | "System" | "Test" | "Timestamp"

	{[Name=string]: #Function & {
		name: Name
	}}
}
