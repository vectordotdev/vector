package metadata

remap: functions: get_env_var: {
	category: "System"
	description: """
		Gets the value of the environment variable specifed by `name`.
		"""

	arguments: [
		{
			name:        "name"
			description: "Name of the environment variable."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"Environment variable `name` does not exist",
		"Value of environment variable `name` is not valid unicode",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Get environment variable"
			source: #"""
				get_env_var("HOME")
				"""#
			return: "/root"
		},
	]
}
