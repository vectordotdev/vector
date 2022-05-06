package metadata

remap: literals: string: {
	title:       "String"
	description: """
		A _string_ literal is a [UTF-8–encoded](\(urls.utf8)) string. String literals can be raw or interpreted.

		**Raw string** literals are composed of the	uninterpreted (implicitly UTF-8-encoded) characters between single
		quotes identified with the `s` sigil and wrapped with single quotes (`s'...'`); in particular, backslashes have
		no special meaning and the string may contain newlines.

		**Interpreted string** literals are character sequences between double quotes (`"..."`). Within the quotes,
		any character may appear except unescaped newline and unescaped double quote. The text between the quotes forms the result
		of the literal, with backslash escapes interpreted as defined below. Strings can be templated by enclosing
		variables in `{{..}}`. The value of the variables are inserted into the string at that position.
		"""

	examples: [
		#"""
			"Hello, world! 🌎"
			"""#,
		#"""
			"Hello, world! \u1F30E"
			"""#,
		#"""
			"Hello, \
			 world!"
			"""#,
		#"""
			"Hello, {{ planet }}!"
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
				"\\u{7FFF}": "24-bit Unicode character code (up to 6 digits)"
				"\\n":       "Newline"
				"\\r":       "Carriage return"
				"\\t":       "Tab"
				"\\\\":      "Backslash"
				"\\0":       "Null"
				"\\\"":      "Double quote"
				"\\'":       "Single quote"
				"\\{":       "Brace"
			}
		}
		templates: {
			title: "Templates"
			description: """
				Strings can be templated by enclosing a variable name with `{{..}}`. The
				value of the variable is inserted into the string at this position at runtime.
				Currently, the variable has to be a string. Only variables are supported, if
				you want to insert a path from the event you must assign it to a variable
				first. To insert a `{{` into the string it can be escaped with a `\\`
				escape: `\\{{..\\}}`. We plan to expand this in future to allow paths and
				format strings to enable non string variables.
				"""
		}
		multiline_strings: {
			title: "Multiline strings"
			description: """
				Long strings can be split over multiple lines by adding a backslash just before the
				newline. The newline and any whitespace at the start of the ensuing line is not
				included in the string.
				"""
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
