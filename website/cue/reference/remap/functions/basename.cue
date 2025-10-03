package metadata

remap: functions: basename: {
	category: "String"
	description: """
		Returns the filename component of the given `path`. This is similar to the Unix `basename` command.
		If the path ends in a directory separator, the function returns the name of the directory.
		"""

	arguments: [
		{
			name:        "value"
			description: "The path from which to extract the basename."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid string.",
	]
	return: types: ["string", "null"]

	examples: [
		{
			title: "Extract basename from file path"
			source: """
				basename!("/usr/local/bin/vrl")
				"""
			return: "vrl"
		},
		{
			title: "Extract basename from file path with extension"
			source: """
				basename!("/home/user/file.txt")
				"""
			return: "file.txt"
		},
		{
			title: "Extract basename from directory path"
			source: """
				basename!("/home/user/")
				"""
			return: "user"
		},
		{
			title: "Root directory has no basename"
			source: """
				basename!("/")
				"""
			return: null
		},
	]
}
