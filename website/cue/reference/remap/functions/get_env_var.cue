package metadata

remap: functions: get_env_var: {
	category: "System"
	description: """
		Returns the value of the environment variable specified by `name`.
		"""

	arguments: [
		{
			name:        "name"
			description: "The name of the environment variable."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"Environment variable `name` does not exist.",
		"The value of environment variable `name` is not valid Unicode",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Get an environment variable"
			source: #"""
				get_env_var!("HOME")
				"""#
			return: "/root"
		},
	]
}
