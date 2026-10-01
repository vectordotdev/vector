package metadata

_schemaDefinitions: "codecs::decoding::format::vrl::VrlDeserializerOptions": object: options: {
	source: {
		description: """
			The [Vector Remap Language][vrl] (VRL) program to execute for each event.
			The final contents of the `.` target are used as the decoding result.
			Compilation errors or use of `abort` in the program result in a decoding error.

			[vrl]: https://vector.dev/docs/reference/vrl
			"""
		required: true
		type: string: {}
	}
	timezone: {
		description: """
			The name of the timezone to apply to timestamp conversions that do not contain an explicit
			time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
			to indicate system local time.

			If not set, `local` is used.

			[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
}
