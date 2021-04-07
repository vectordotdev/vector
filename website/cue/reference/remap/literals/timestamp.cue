package metadata

remap: literals: timestamp: {
	title:       "Timestamp"
	description: """
		A _timestamp_ literal defines a native timestamp expressed in the [RFC 3339 format](\(urls.rfc_3339)) with a
		nanosecond precision.

		Timestamp literals are defined by the `t` sigil and wrapped with single quotes (`t'2021-02-11T10:32:50.553955473Z'`).
		"""

	examples: [
		#"""
			t'2021-02-11T10:32:50.553955473Z'
			"""#,
		#"""
			t'2021-02-11T10:32:50.553Z'
			"""#,
		#"""
			t'2021-02-11T10:32:50.553-04:00'
			"""#,
	]

	characteristics: {
		timezones: {
			title:       "Timezones"
			description: """
				As defined in [RFC 3339 format](\(urls.rfc_3339)), timestamp literals support UTC and local offsets.
				"""
		}
	}
}
