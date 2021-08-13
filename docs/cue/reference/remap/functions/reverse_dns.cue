package metadata

remap: functions: reverse_dns: {
	category: "IP"
	description: #"""
		Performs a reverse DNS lookup on the given IP address. Note this function has the potential to reduce
		performance as it may have to perform a network call each time.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The ip address to look up."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid IP address",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Google"
			source: #"""
				reverse_dns("8.8.8.8")
				"""#
			return: "dns.google.com"
		},
	]
}
