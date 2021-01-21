package metadata

remap: literals: regular_expression: {
	title:       "Regular Expression"
	description: """
		A _regular expression_ literal represents a [Regular Expression](\(urls.regex)) used for string matching and
		parsing.

		Regular expressions are delimited by `/` and use [Rust regex syntax](\(urls.rust_regex_syntax)). There is one
		limitation with Regular Expressions in VRL:

		You can't assign a regex to a map path. Thus, `.pattern = /foo|bar/i` is not allowed. This is because regex's
		cannot be serialized to JSON.
		"""

	examples: [
		#"/^Hello, World!$/"#,
		#"/^Hello, World!$/i"#,
		#"/^\d{4}-\d{2}-\d{2}$/"#,
		#"/(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})/"#,
	]

	characteristics: {
		flags: {
			title:       "Flags"
			description: #"""
				Regular expressions allow three flags:

				|Flag | Description
				|:----|:-----------
				|`x`  | Ignore whitespace
				|`i`  | Case insensitive
				|`m`  | Multi-line mode

				Regex flags can be combined, as in `/pattern/xmi`, `/pattern/im`, etc.

				To learn more about regular expressions in Rust—and by extension in VRL—we strongly
				recommend the in-browser [Rustexp expression editor and
				tester](\#(urls.regex_tester)).
				"""#
		}
		named_captures: {
			title: "Named Captures"
			description: #"""
				Regular Expressions support named capture groups, allowing extractions to be keyed by their name.
				Named captures should be preceded with a `?P<name>` declaraction. For example:

				```js
				/(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})/
				```

				Will extract captures with the `y`, `m`, and `d` keys.
				"""#
		}
	}
}
