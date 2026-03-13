package metadata

remap: {
	#Argument: {
		name:        string
		description: string
		required:    bool
		default?: bool | string | int | [string, ...string]
		type: [remap.#Type, ...remap.#Type]
		enum?: #Enum
	}

	#Function: {
		anchor:      name
		name:        string
		category:    #FunctionCategory
		description: string
		notices: [string, ...string] | *[]

		arguments: [...#Argument]
		return: {
			types: [remap.#Type, ...remap.#Type]
			rules?: [string, ...string]
		}
		internal_failure_reasons: [...string]
		examples?: [remap.#FunctionExample, ...remap.#FunctionExample]
		deprecated: bool | *false
		pure:       bool | *true
	}

	function_categories: [
		"Array",
		"Codec",
		"Coerce",
		"Convert",
		"Debug",
		"Enrichment",
		"Enumerate",
		"Event",
		"Path",
		"Cryptography",
		"IP",
		"Map",
		"Metrics",
		"Number",
		"Object",
		"Parse",
		"Random",
		"String",
		"System",
		"Timestamp",
		"Type",
		"Checksum",
	]

	#FunctionCategory: or(function_categories)

	functions: [Name=string]: #Function & {
		name: Name
	}
}
