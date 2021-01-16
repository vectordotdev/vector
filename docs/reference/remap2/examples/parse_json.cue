package metadata

remap2: examples: parse_json: {
	title: "Parse JSON"
	input: log: message: "{\"Hello\": \"World!\"}"
	source: #"""
		., err = parse_json(del(.message))
		"""#
	output: log: Hello: "World!"
}
