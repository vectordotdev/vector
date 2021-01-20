package metadata

remap: {
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
		name:        string
		category:    #FunctionCategory
		description: string
		notices:     [string, ...string] | *[]

		arguments: [...#Argument]
		return: [remap.#Type, ...remap.#Type]
		internal_failure_reasons: [...string]
		examples?: [remap.#Example, ...remap.#Example]
	}

	#FunctionCategory: "Array" | "Codec" | "Coerce" | "Debug" | "Enumerate" | "Event" | "Hash" | "IP" | "Map" | "Number" | "Parse" | "Random" | "String" | "System" | "Timestamp" | "Type"

	functions: [Name=string]: #Function & {
		name: Name
	}
}
