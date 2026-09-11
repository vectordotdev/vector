package metadata

_schemaDefinitions: "core::option::Option<vector::sources::util::multiline_config::MultilineConfig>": object: options: {
	condition_pattern: {
		description: """
			Regular expression pattern that is used to determine whether or not more lines should be read.

			This setting must be configured in conjunction with `mode`.
			"""
		required: true
		type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
	}
	mode: {
		description: """
			Aggregation mode.

			This setting must be configured in conjunction with `condition_pattern`.
			"""
		required: true
		type: string: enum: {
			continue_past: """
				All consecutive lines matching this pattern, plus one additional line, are included in the group.

				This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
				that the following line is part of the same message.
				"""
			continue_through: """
				All consecutive lines matching this pattern are included in the group.

				The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.

				This is useful in cases such as a Java stack trace, where some indicator in the line (such as a leading
				whitespace) indicates that it is an extension of the proceeding line.
				"""
			halt_before: """
				All consecutive lines not matching this pattern are included in the group.

				This is useful where a log line contains a marker indicating that it begins a new message.
				"""
			halt_with: """
				All consecutive lines, up to and including the first line matching this pattern, are included in the group.

				This is useful where a log line ends with a termination marker, such as a semicolon.
				"""
		}
	}
	start_pattern: {
		description: "Regular expression pattern that is used to match the start of a new message."
		required:    true
		type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
	}
	timeout_ms: {
		description: """
			The maximum amount of time to wait for the next additional line, in milliseconds.

			Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
			"""
		required: true
		type: uint: {
			examples: [1000, 600000]
			unit: "milliseconds"
		}
	}
}
