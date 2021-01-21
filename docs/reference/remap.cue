package metadata

#Remap: {
	#Characteristic: {
		title:       string
		description: string
	}

	#Example: {
		title:   string
		input?:  #Event
		source:  string
		raises?: string

		if raises == _|_ {
			return?: _
			output?: #Event
		}
	}

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "path" | "string" | "regex" | "timestamp"

	concepts:    _
	description: string
	example:     string
	expressions: _
	features:    _
	functions:   _
	literals:    _
	principles:  _
}

remap: #Remap & {
	description: #"""
		**Vector Remap Language** (VRL) is an [expression-oriented](\#(urls.expression_oriented_language)) language
		designed for expressing obervability data (logs and metrics) transformations. It features a simple
		[syntax](\#(urls.vrl_spec)) and a rich set of built-in [functions](\#(urls.vrl_functions)) tailored
		specifically to observability use cases.

		For a more in-depth picture, see the [announcement blog post](\#(urls.vrl_announcement)) for more details.
		"""#

	example: #"""
		The following program parses, shapes, generates an ID, and conerces a timestamp in 4 lines:

		```vrl title="example VRL program"
		. = parse_syslog!(.message)
		.severity = "info"
		.id = uuid_v4()
		.timestamp = to_int(.timestamp)
		```
		"""#
}
