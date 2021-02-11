package metadata

remap: literals: string: {
	title:       "String"
	description: """
		A _string_ literal is a [UTF-8–encoded](\(urls.utf8)) string. String literals can be raw or interpreted.

		**Raw string** literals are composed of the	uninterpreted (implicitly UTF-8-encoded) characters between single
		quotes identified with the `s` sigil and wrapped with single quotes (`s'...'`); in particular, backslashes have
		no special meaning and the string may contain newlines.

		**Interpreted string** literals are character sequences between double quotes (`"..."`). Within the quotes,
		any character may appear except newline and unescaped double quote. The text between the quotes forms the result
		of the literal, with backslash escapes interpreted as defined below.
		"""

	examples: [
		#"""
			"Hello, world! 🌎"
			"""#,
		#"""
			"Hello, world! \\u1F30E"
			"""#,
		#"""
			s'Hello, world!'
			"""#,
		#"""
			s'{ "foo": "bar" }'
			"""#,
	]

	characteristics: {
		backslash_escapes: {
			title: "Backslash escapes"
			description: """
				Special characters, such as newlines, can be expressed with a backslash escape.
				"""
			enum: {
				"`\\u{7FFF}`": "24-bit Unicode character code (up to 6 digits)"
				"`\\n`":       "Newline"
				"`\\r`":       "Carriage return"
				"`\\t`":       "Tab"
				"`\\\\`":      "Backslash"
				"`\\0`":       "Null"
				"`\\\"`":      "Double quote"
				"`\\'`":       "Single quote"
			}
		}
		concatenation: {
			title: "Concatenation"
			description: """
				Strings can be concatenated with the `+` operator.
				"""
		}
		invalid_characters: {
			title: "Invalid Characters"
			description: """
				Invalid UTF-8 sequences are replaced with the `�` character.
				"""
		}
	}
}
