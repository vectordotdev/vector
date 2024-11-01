package metadata

remap: functions: parse_aws_vpc_flow_log: {
	category:    "Parse"
	description: """
		Parses `value` in the [VPC Flow Logs format](\(urls.aws_vpc_flow_logs)).
		"""

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
		"`value` is not a properly formatted AWS VPC Flow log.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse AWS VPC Flow log (default format)"
			source: #"""
				parse_aws_vpc_flow_log!("2 123456789010 eni-1235b8ca123456789 - - - - - - - 1431280876 1431280934 - NODATA")
				"""#
			return: {
				"version":      2
				"account_id":   "123456789010"
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
				parse_aws_vpc_flow_log!(
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
		{
			title: "Parse AWS VPC Flow log including v5 fields"
			source: #"""
				parse_aws_vpc_flow_log!("5 52.95.128.179 10.0.0.71 80 34210 6 1616729292 1616729349 IPv4 14 15044 123456789012 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-0c50d5961bcb2d47b eni-1235b8ca123456789 ap-southeast-2 apse2-az3 - - ACCEPT 19 52.95.128.179 10.0.0.71 S3 - - ingress OK",
				format: "version srcaddr dstaddr srcport dstport protocol start end type packets bytes account_id vpc_id subnet_id instance_id interface_id region az_id sublocation_type sublocation_id action tcp_flags pkt_srcaddr pkt_dstaddr pkt_src_aws_service pkt_dst_aws_service traffic_path flow_direction log_status")
				"""#
			return: {
				"account_id":          "123456789012"
				"action":              "ACCEPT"
				"az_id":               "apse2-az3"
				"bytes":               15044
				"dstaddr":             "10.0.0.71"
				"dstport":             34210
				"end":                 1616729349
				"flow_direction":      "ingress"
				"instance_id":         "i-0c50d5961bcb2d47b"
				"interface_id":        "eni-1235b8ca123456789"
				"log_status":          "OK"
				"packets":             14
				"pkt_dst_aws_service": null
				"pkt_dstaddr":         "10.0.0.71"
				"pkt_src_aws_service": "S3"
				"pkt_srcaddr":         "52.95.128.179"
				"protocol":            6
				"region":              "ap-southeast-2"
				"srcaddr":             "52.95.128.179"
				"srcport":             80
				"start":               1616729292
				"sublocation_id":      null
				"sublocation_type":    null
				"subnet_id":           "subnet-aaaaaaaa012345678"
				"tcp_flags":           19
				"traffic_path":        null
				"type":                "IPv4"
				"version":             5
				"vpc_id":              "vpc-abcdefab012345678"
			}
		},
	]
}
