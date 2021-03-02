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
		anchor:      name
		name:        string
		category:    #FunctionCategory
		description: string
		notices:     [string, ...string] | *[]

		arguments: [...#Argument]
		return: {
			types: [remap.#Type, ...remap.#Type]
			rules?: [string, ...string]
		}
		internal_failure_reasons: [...string]
		examples?: [remap.#Example, ...remap.#Example]
	}

	#FunctionCategory: "Array" | "Codec" | "Coerce" | "Debug" | "Enumerate" | "Event" | "Hash" | "IP" | "Number" | "Object" | "Parse" | "Random" | "String" | "System" | "Timestamp" | "Type"

	functions: [Name=string]: #Function & {
		name: Name
	}
}
