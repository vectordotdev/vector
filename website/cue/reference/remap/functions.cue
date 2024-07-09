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
		examples?: [remap.#Example, ...remap.#Example]
		deprecated: bool | *false
		pure:       bool | *true
	}

	#FunctionCategory: "Array" | "Codec" | "Coerce" | "Convert" | "Debug" | "Enrichment" | "Enumerate" | "Event" | "Path" | "Cryptography" | "IP" | "Number" | "Object" | "Parse" | "Random" | "String" | "System" | "Timestamp" | "Type"

	// A helper array for generating docs. At some point, we should generate this from the
	// #FunctionCategory enum if CUE adds support for that.
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

	// Reusable text
	_enrichment_table_explainer: """
		For `file` enrichment tables, this condition needs to be a VRL object in which
		the key-value pairs indicate a field to	search mapped to a value to search in that field.
		This function returns the rows that match the provided condition(s). _All_ fields need to
		match for rows to be returned; if any fields do not match, then no rows are returned.

		There are currently two forms of search criteria:

		1. **Exact match search**. The given field must match the value exactly. Case sensitivity
		   can be specified using the `case_sensitive` argument. An exact match search can use an
		   index directly into the dataset, which should make this search fairly "cheap" from a
		   performance perspective.

		2. **Date range search**. The given field must be greater than or equal to the `from` date
		   and less than or equal to the `to` date. A date range search involves
		   sequentially scanning through the rows that have been located using any exact match
		   criteria. This can be an expensive operation if there are many rows returned by any exact
		   match criteria. Therefore, use date ranges as the _only_ criteria when the enrichment
		   data set is very small.

		For `geoip` and `mmdb` enrichment tables, this condition needs to be a VRL object with a single key-value pair
		whose value needs to be a valid IP address. Example: `{"ip": .ip }`. If a return field is expected
		and without a value, `null` is used. This table can return the following fields:

		* ISP databases:
			* `autonomous_system_number`
			* `autonomous_system_organization`
			* `isp`
			* `organization`

		* City databases:
			* `city_name`
			* `continent_code`
			* `country_code`
			* `country_name`
			* `region_code`
			* `region_name`
			* `metro_code`
			* `latitude`
			* `longitude`
			* `postal_code`
			* `timezone`

		* Connection-Type databases:
			* `connection_type`

		To use this function, you need to update your configuration to
		include an
		[`enrichment_tables`](\(urls.vector_configuration_global)/#enrichment_tables)
		parameter.
		"""
}
