package metadata

remap: {
	#Argument: {
		name:        string
		description: string
		required:    bool
		default?:    bool | string | int | [string, ...string]
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

	#FunctionCategory: "Array" | "Codec" | "Coerce" | "Convert" | "Debug" | "Enrichment" | "Enumerate" | "Path" | "Hash" | "IP" | "Number" | "Object" | "Parse" | "Random" | "String" | "System" | "Timestamp" | "Type"

	// A helper array for generating docs. At some point, we should generate this from the
	// #FunctionCategory enum if CUE adds support for that.
	function_categories: [
		"Array",
		"Codec",
		"Coerce",
		"Convert",
		"Debug",
		"Enumerate",
		"Path",
		"Hash",
		"IP",
		"Number",
		"Object",
		"Parse",
		"Random",
		"String",
		"System",
		"Timestamp",
		"Type",
	]

	functions: [Name=string]: #Function & {
		name: Name
	}
}
