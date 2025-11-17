package metadata

remap: functions: dirname: {
	category: "String"
	description: """
		Returns the directory component of the given `path`. This is similar to the Unix `dirname` command.
		The directory component is the path with the final component removed.
		"""

	arguments: [
		{
			name:        "value"
			description: "The path from which to extract the directory name."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Extract dirname from file path"
			source: """
				dirname!("/usr/local/bin/vrl")
				"""
			return: "/usr/local/bin"
		},
		{
			title: "Extract dirname from file path with extension"
			source: """
				dirname!("/home/user/file.txt")
				"""
			return: "/home/user"
		},
		{
			title: "Extract dirname from directory path"
			source: """
				dirname!("/home/user/")
				"""
			return: "/home"
		},
		{
			title: "Root directory dirname is itself"
			source: """
				dirname!("/")
				"""
			return: "/"
		},
		{
			title: "Relative files have current directory as dirname"
			source: """
				dirname!("file.txt")
				"""
			return: "."
		},
	]
}
