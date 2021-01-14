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
	return: ["string"]
	category: "System"
	description: #"""
		Get an environment variable. If not exists, a `Call` error will be raised.
		"""#
	examples: [
		{
			title: "Get environment variable"
			input: log: {}
			source: #"""
				.home = get_env_var!("HOME")
				.not_found = get_env_var("SOME_VAR") ?? "default"
				"""#
			output: log: {
				home:      "/root"
				not_found: "default"
			}
		},
	]
}
