package metadata

remap: functions: community_id: {
	category:    "String"
	description: """
		Generates an ID based on the [Community ID Spec](\(urls.community_id_spec)).
		"""

	arguments: [
		{
			name:        "source_ip"
			description: "The source IP address."
			required:    true
			type: ["string"]
		},
		{
			name:        "destination_ip"
			description: "The destination IP address."
			required:    true
			type: ["string"]
		},
		{
			name:        "protocol"
			description: "The protocol number."
			required:    true
			type: ["integer"]
		},
		{
			name:        "source_port"
			description: "The source port."
			required:    false
			type: ["integer"]
		},
		{
			name:        "destination_port"
			description: "The destination port."
			required:    false
			type: ["integer"]
		},
		{
			name:        "seed"
			description: "The custom seed number."
			required:    false
			type: ["integer"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "TCP"
			source: #"""
				community_id!(source_ip: "1.2.3.4", destination_ip: "5.6.7.8", source_port: 1122, destination_port: 3344, protocol: 6)
				"""#
			return: "1:wCb3OG7yAFWelaUydu0D+125CLM="
		},
	]
}
