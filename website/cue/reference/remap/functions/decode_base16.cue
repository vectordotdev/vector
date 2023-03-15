package metadata

remap: functions: decode_base16: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Base16](\(urls.base16)) string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Base16](\(urls.base16)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Base16 string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Base16 data"
			source: """
				decode_base16!("796f752068617665207375636365737366756c6c79206465636f646564206d65")
				"""
			return: "you have successfully decoded me"
		},
	]
}
