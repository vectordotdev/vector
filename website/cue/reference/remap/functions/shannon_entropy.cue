package metadata

remap: functions: shannon_entropy: {
	category:    "String"
	description: """
		Generates [Shannon entropy](\(urls.shannon_entropy)) from given string. It can generate it
		based on string bytes, codepoints, or graphemes.
		"""

	arguments: [
		{
			name:        "value"
			description: "The input string."
			required:    true
			type: ["string"]
		},
		{
			name: "segmentation"
			description: """
				Defines how to split the string to calculate entropy, based on occurrences of
				segments.

				Byte segmentation is the fastest, but it might give undesired results when handling
				UTF-8 strings, while grapheme segmentation is the slowest, but most correct in these
				cases.
				"""
			required: false
			type: ["string"]
			default: "byte"
			enum: {
				byte:      "Considers individual bytes when calculating entropy"
				codepoint: "Considers codepoints when calculating entropy"
				grapheme:  "Considers graphemes when calculating entropy"
			}
		},
	]
	internal_failure_reasons: []
	return: types: ["float"]

	examples: [
		{
			title: "Simple byte segmentation example"
			source: #"""
				floor(shannon_entropy("vector.dev"), precision: 4)
				"""#
			return: 2.9219
		},
		{
			title: "UTF-8 string with bytes segmentation"
			source: #"""
				floor(shannon_entropy("test123%456.فوائد.net."), precision: 4)
				"""#
			return: 4.0784
		},
		{
			title: "UTF-8 string with grapheme segmentation"
			source: #"""
				floor(shannon_entropy("test123%456.فوائد.net.", segmentation: "grapheme"), precision: 4)
				"""#
			return: 3.9362
		},
	]
}
