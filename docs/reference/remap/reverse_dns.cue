package metadata

remap: functions: reverse_dns: {
	arguments: [
		{
			name:        "ip"
			description: "The ip address to look up."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "networking"
	description: #"""
		Performs a reverse DNS lookup on the given IP address. Note this function has the potential to reduce
		performance as it may have to perform a network call each time.
		"""#
	examples: [
		{
			title: "Google"
			input: {
				ip: "8.8.8.8"
			}
			source: #"""
				.host = reverse_dns(ip = .ip)
				"""#
			output: {
				ip: "8.8.8.8"
				host: "dns.google"
			}
		},
	]
}
