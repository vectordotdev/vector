package metadata

remap: functions: decode_zstd: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Zstandard](\(urls.zstd)) string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Zstandard](\(urls.zstd)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Zstd string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Zstd data"
			source: #"""
				encoded_text = decode_base64!("KLUv/QBY/QEAYsQOFKClbQBedqXsb96EWDax/f/F/z+gNU4ZTInaUeAj82KqPFjUzKqhcfDqAIsLvAsnY1bI/N2mHzDixRQA")
				decode_zstd!(encoded_text)
				"""#
			return: "you_have_successfully_decoded_me.congratulations.you_are_breathtaking."
		},
	]
}
