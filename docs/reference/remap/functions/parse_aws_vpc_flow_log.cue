package metadata

remap: functions: parse_aws_vpc_flow_log: {
	arguments: [
		{
			name:        "value"
			description: "VPC Flow Log."
			required:    true
			type: ["string"]
		},
		{
			name:        "format"
			description: "VPC Flow Log format."
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted AWS VPC Flow log",
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses [VPC Flow Logs]\(urls.aws_vpc_flow_logs\) into it's constituent components.
		"""#
	examples: [
		{
			title: "Parse AWS VPC Flow log (default format)"
			source: #"""
				parse_aws_vpc_flow_log("2 123456789010 eni-1235b8ca123456789 - - - - - - - 1431280876 1431280934 - NODATA")
				"""#
			return: {
				"version":      2
				"account_id":   123456789010
				"interface_id": "eni-1235b8ca123456789"
				"srcaddr":      null
				"dstaddr":      null
				"srcport":      null
				"dstport":      null
				"protocol":     null
				"packets":      null
				"bytes":        null
				"start":        1431280876
				"end":          1431280934
				"action":       null
				"log_status":   "NODATA"
			}
		},
		{
			title: "Parse AWS VPC Flow log (custom format)"
			source: #"""
				parse_aws_vpc_flow_log(
					"- eni-1235b8ca123456789 10.0.1.5 10.0.0.220 10.0.1.5 203.0.113.5",
					"instance_id interface_id srcaddr dstaddr pkt_srcaddr pkt_dstaddr"
				)
				"""#
			return: {
				"instance_id":  null
				"interface_id": "eni-1235b8ca123456789"
				"srcaddr":      "10.0.1.5"
				"dstaddr":      "10.0.0.220"
				"pkt_srcaddr":  "10.0.1.5"
				"pkt_dstaddr":  "203.0.113.5"
			}
		},
	]
}
