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
		specifically to observability use cases. For example, the following script parses, shapes, generates an ID,
		and converces a timestamp in 4 lines:

		```vrl title="example VRL program"
		# Parse into named fields, and abort if parsing fails
		. = parse_syslog!(.message)
		# Overwrite the severity
		.severity = "info"
		# Add a unique ID to the event
		.id = uuid_v4()
		# Convert the timestamp to seconds since the Unix epoch
		.timestamp = to_int(.timestamp)
		```

		For a more in-depth picture, see the [announcement blog post](\#(urls.vrl_announcement)) for more details.
		"""#
}
