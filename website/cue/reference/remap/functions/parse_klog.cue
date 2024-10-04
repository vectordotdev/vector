package metadata

remap: functions: parse_klog: {
	category:    "Parse"
	description: """
		Parses the `value` using the [klog](\(urls.klog)) format used by Kubernetes components.
		"""
	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` does not match the `klog` format.",
	]
	return: types: ["object"]
	examples: [
		{
			title: "Parse using klog"
			source: #"""
				parse_klog!("I0505 17:59:40.692994   28133 klog.go:70] hello from klog")
				"""#
			return: {
				file:      "klog.go"
				id:        28133
				level:     "info"
				line:      70
				message:   "hello from klog"
				timestamp: "2024-05-05T17:59:40.692994Z"
			}
		},
	]
}
