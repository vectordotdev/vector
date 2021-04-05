package metadata

remap: functions: get_hostname: {
	category: "System"
	description: """
		Returns the first IP of the first non-loopback interface that is up. It
		can return either an IPv4 or IPv6 address and returns `null` if it
		cannot find any valid IP.
		"""

	arguments: []
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Get host IP"
			input: log: {}
			source: #"""
				.host_ip = get_host_ip()
				"""#
			output: log: host_ip: "172.22.0.1"
		},
	]
}
