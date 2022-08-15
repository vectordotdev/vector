package metadata

remap: functions: parse_aws_clb_log: {
	category:    "Parse"
	description: """
		Parses `value` in the [AWS Classic Elastic Load Balancer Access format](\(urls.aws_clb_access_format)).
		"""

	arguments: [
		{
			name:        "value"
			description: "Access log of the AWS Classic Load Balancer."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a properly formatted AWS CLB log",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse AWS CLB HTTP log"
			source: #"""
				parse_aws_clb_log!(
					"2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.000073 0.001048 0.000057 200 200 0 29 \"GET http://www.example.com:80/ HTTP/1.1\" \"curl/7.38.0\" - -"
				)
				"""#
			return: {
				time:                     "2015-05-13T23:39:43.945958Z"
				elb:                      "my-loadbalancer"
				client_host:              "192.168.131.39:2817"
				target_host:              "10.0.0.1:80"
				request_processing_time:  0.000073
				backend_processing_time:  0.001048
				response_processing_time: 0.000057
				elb_status_code:          "200"
				backend_status_code:      "200"
				received_bytes:           0
				sent_bytes:               29
				request_method:           "GET"
				request_url:              "http://www.example.com:80/"
				request_protocol:         "HTTP/1.1"
				user_agent:               "curl/7.38.0"
				ssl_cipher:               null
				ssl_protocol:             null
			}
		},
	]
}
