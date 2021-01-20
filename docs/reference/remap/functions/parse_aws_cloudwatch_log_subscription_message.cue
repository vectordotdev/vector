package metadata

remap: functions: parse_aws_cloudwatch_log_subscription_message: {
	arguments: [
		{
			name:        "value"
			description: "The string representation of the message to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted AWS Cloudwatch Log subscription message",
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses AWS CloudWatch Logs events (configured through AWS Cloudwatch
		subscriptions) coming from the `aws_kinesis_firehose` source.
		"""#
	examples: [
		{
			title: "Parse AWS Cloudwatch Log subscription message"
			input: log: message: """
				{
				  "messageType": "DATA_MESSAGE",
				  "owner": "111111111111",
				  "logGroup": "test",
				  "logStream": "test",
				  "subscriptionFilters": [
					"Destination"
				  ],
				  "logEvents": [
					{
					  "id": "35683658089614582423604394983260738922885519999578275840",
					  "timestamp": 1600110569039,
					  "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
					}
				  ]
				}
				"""
			source: #"""
				parse_aws_cloudwatch_log_subscription_message(.message)
				"""#
			output: log: {
				owner:        "111111111111"
				message_type: "DATA_MESSAGE"
				log_group:    "test"
				log_stream:   "test"
				subscription_filters: [ "Destination"]
				log_events: [{
					id:        "35683658089614582423604394983260738922885519999578275840"
					message:   "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-latform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
					timestamp: "2020-09-14T19:09:29.039Z"
				}]
			}
		},
	]
}
