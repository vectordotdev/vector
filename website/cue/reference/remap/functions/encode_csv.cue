package metadata

remap: functions: encode_csv: {
	category: "Codec"
	description: """
        Encodes the `value` to a single CSV formatted row.
        """

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a CSV string."
			required:    true
			type: ["any"]
		},
		{
			name:        "delimiter"
			description: "The field delimiter to use when encoding. Must be a single-byte UTF-8 character."
			required:    false
			default:     ","
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"The delimiter must be a single-byte UTF-8 character.",
		"`value` is not an object convertible to a CSV string.",
		"The `csv` crate encountered an I/O error while writing or flushing the output.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encode object to a single CSV formatted row"
			source: #"encode_csv!(["foo","bar","foo \", bar"])"#
			return: #""foo,bar,\"foo \"\", bar\"""#
		},
		{
			title: "Encode object to a single CSV formatted row with custom delimiter",
			source: #"encode_csv!(["foo","bar"], delimiter: " ")"#
			return: #""foo bar""#
		},
		{
			title: "Encode object to a single CSV formatted row with line breaks"
			source: #"encode_csv!(["line", "with line breaks", "here\n", "and", "\nhere"])"#
			return: #""line,with line breaks,\"here\n\",and,\"\nhere\"""#
		},
	]
}
