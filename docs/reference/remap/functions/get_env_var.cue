package metadata

remap: functions: get_env_var: {
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
	return: ["string"]
	category: "System"
	description: #"""
		Get the value of an environment variable. If the variable does not exists, an error is returned.
		"""#
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
