package metadata

_schemaDefinitions: "codecs::encoding::format::csv::CsvSerializerOptions": object: options: {
	capacity: {
		description: """
			Sets the capacity (in bytes) of the internal buffer used in the CSV writer.
			This defaults to 8192 bytes (8KB).
			"""
		required: false
		type: uint: default: 8192
	}
	delimiter: {
		description: "The field delimiter to use when writing CSV."
		required:    false
		type: ascii_char: default: ","
	}
	double_quote: {
		description: """
			Enables double quote escapes.

			This is enabled by default, but you can disable it. When disabled, quotes in
			field data are escaped instead of doubled.
			"""
		required: false
		type: bool: default: true
	}
	escape: {
		description: """
			The escape character to use when writing CSV.

			In some variants of CSV, quotes are escaped using a special escape character
			like \\ (instead of escaping quotes by doubling them).

			To use this, `double_quotes` needs to be disabled as well; otherwise, this setting is ignored.
			"""
		required: false
		type: ascii_char: default: "\""
	}
	fields: {
		description: """
			Configures the fields that are encoded, as well as the order in which they
			appear in the output.

			If a field is not present in the event, the output for that field is an empty string.

			Values of type `Array`, `Object`, and `Regex` are not supported, and the
			output for any of these types is an empty string.
			"""
		required: true
		type: array: items: type: string: {}
	}
	quote: {
		description: "The quote character to use when writing CSV."
		required:    false
		type: ascii_char: default: "\""
	}
	quote_style: {
		description: "The quoting style to use when writing CSV data."
		required:    false
		type: string: {
			default: "necessary"
			enum: {
				always: "Always puts quotes around every field."
				necessary: """
					Puts quotes around fields only when necessary.
					They are necessary when fields contain a quote, delimiter, or record terminator.
					Quotes are also necessary when writing an empty record
					(which is indistinguishable from a record with one empty field).
					"""
				never: "Never writes quotes, even if it produces invalid CSV data."
				non_numeric: """
					Puts quotes around all fields that are non-numeric.
					This means that when writing a field that does not parse as a valid float or integer,
					quotes are used even if they aren't strictly necessary.
					"""
			}
		}
	}
}
