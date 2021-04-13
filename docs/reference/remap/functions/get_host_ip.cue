package metadata

remap: functions: get_hostname: {
	category: "System"
	description: """
		Returns the first discoverable IP of the host Vector is running on.

		The search for the IP can be scoped by the `interface`. Otherwise, Vector
		will find the first non-loopback interface that is up.

		It will return either an IPv4 or IPv6 address by default, but can be
		scoped to look for one or the other via `family`.
		"""

	arguments: [
		{
			name: "interface"
			description: """
				The network interface to pull the first IP from. Otherwise the first non-loopback interface that is up will be chosen.
				"""
			required: false
			default:  null
			type: ["string"]
		},
		{
			name:        "family"
			description: "The address family to scope the IP search to."
			enum: {
				"IPV4": "IPv4 addresses"
				"IPv6": "IPv6 addresses"
			}
			required: false
			default:  null
			type: ["string"]
		},
	]
	internal_failure_reasons: ["unable to find IP address"]
	return: types: ["string"]

	examples: [
		{
			title: "Get host IP"
			input: log: {}
			source: #"""
				.host_ip = get_host_ip!()
				"""#
			output: log: host_ip: "172.22.0.1"
		},
		{
			title: "Get host IP, but only IPv4"
			input: log: {}
			source: #"""
				.host_ip = get_host_ip(family: "IPv4")
				"""#
			output: log: host_ip: "172.22.0.1"
		},
		{
			title: "Get IP of eth0"
			input: log: {}
			source: #"""
				.host_ip = get_host_ip(interface: "eth0")
				"""#
			output: log: host_ip: "172.22.0.1"
		},
	]
}
