package metadata

remap: functions: parse_bytes: {
	category: "Parse"
	description: """
		Parses the `value` into a human-readable bytes format specified by `unit` and `base`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string of the duration with either binary or SI unit."
			required:    true
			type: ["string"]
		},
		{
			name:        "unit"
			description: "The output units for the byte."
			required:    true
			type: ["string"]
			enum: {
				B:   "Bytes"
				kiB: "Kilobytes (1024 bytes)"
				MiB: "Megabytes (1024 ** 2 bytes)"
				GiB: "Gigabytes (1024 ** 3 bytes)"
				TiB: "Terabytes (1024 gigabytes)"
				PiB: "Petabytes (1024 ** 2 gigabytes)"
				EiB: "Exabytes (1024 ** 3 gigabytes)"
				kB:  "Kilobytes (1 thousand bytes in SI)"
				MB:  "Megabytes (1 million bytes in SI)"
				GB:  "Gigabytes (1 billion bytes in SI)"
				TB:  "Terabytes (1 thousand gigabytes in SI)"
				PB:  "Petabytes (1 million gigabytes in SI)"
				EB:  "Exabytes (1 billion gigabytes in SI)"
			}
		},
		{
			name:        "base"
			description: "The base for the byte, either 2 or 10."
			required:    false
			type: ["string"]
			default: 2
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted bytes.",
	]
	return: types: ["float"]

	examples: [
		{
			title: "Parse bytes (kilobytes)"
			source: #"""
				parse_bytes!("1024KiB", unit: "MiB")
				"""#
			return: 1.0
		},
		{
			title: "Parse bytes in SI unit (terabytes)"
			source: #"""
				parse_bytes!("4TB", unit: "MB", base: "10")
				"""#
			return: 4000000.0
		},
		{
			title: "Parse bytes in ambiguous unit (gigabytes)"
			source: #"""
				parse_bytes!("1GB", unit: "B", base: "2")
				"""#
			return: 1073741824.0
		},
	]
}
