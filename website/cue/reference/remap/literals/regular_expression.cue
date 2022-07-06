package metadata

remap: literals: regular_expression: {
	title:       "Regular Expression"
	description: """
		A _regular expression_ literal represents a [Regular Expression](\(urls.regex)) used for string matching and
		parsing.

		Regular expressions are defined by the `r` sigil and wrapped with single quotes (`r'...'`). The value between
		the quotes uses the [Rust regex syntax](\(urls.rust_regex_syntax)).
		"""

	examples: [
		#"r'^Hello, World!$'"#,
		#"r'(?i)^Hello, World!$'"#,
		#"r'^\d{4}-\d{2}-\d{2}$'"#,
		#"r'(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})'"#,
	]

	characteristics: {
		flags: {
			title:       "Flags"
			description: #"""
				Regular expressions allow for flags. Flags can be combined, as in `r'(?ixm)pattern'`,
				`r'(?im)pattern'`, etc.

				To learn more about regular expressions in Rust—and by extension in VRL—we strongly	recommend the
				in-browser [Rustexp expression editor and tester](\#(urls.regex_tester)).
				"""#
			enum: {
				"i": "Case insensitive"
				"m": "Multi-line mode"
				"x": "Ignore whitespace"
				"s": "Allow . to match \n"
				"U": "Swap the meaning of x* and x*?"
				"u": "Unicode support (enabled by default)"
			}
		}
		named_captures: {
			title: "Named Captures"
			description: #"""
				Regular expressions support named capture groups, allowing extractions to be associated with keys.
				Named captures should be preceded with a `?P<name>` declaration. This regex, for example...

				```coffee
				r'(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})'
				```

				...extracts captures with the `y`, `m`, and `d` keys.
				"""#
		}
	}
}
