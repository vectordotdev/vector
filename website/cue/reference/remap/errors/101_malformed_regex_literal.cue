package metadata

remap: errors: "101": {
	title:       "Malformed regex literal"
	description: """
		A [regex literal expression](\(urls.vrl_expressions)#regular-expression) is malformed
		and thus doesn't result in a valid regular expression.
		"""
	rationale: """
		Invalid regular expressions don't compile.
		"""
	resolution: """
		Regular expressions are difficult to write and commonly result in syntax errors. If you're parsing a common
		log format we recommend using one of VRL's [`parse_*` functions](\(urls.vrl_functions)/#parse-functions). If
		you don't see a function for your format please [request it](\(urls.new_feature_request)). Otherwise, use the
		[Rust regex tester](\(urls.regex_tester)) to test and correct your regular expression.
		"""

	examples: [
		{
			"title": "\(title) (common format)"
			source: #"""
				. |= parse_regex!(.message, r'^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$')
				"""#
			diff: #"""
				-. |= parse_regex!(.message, r'^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$')
				+. |= parse_common_log!(.message)
				"""#
		},
	]
}
