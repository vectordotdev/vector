package metadata

remap: functions: parse_aws_cloudtrail_logs: {
	category: "Parse"
	description: """
		Parses AWS CloudTrail log records used to monitor account events. Trails are saved to an Amazon S3 bucket and can be read via the AWS S3 source.
		"""
	arguments: [
		{
			name:        "value"
			description: "The string representation of the message to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a properly formatted AWS CloudTrail log",
	]
	return: types: ["array"]
	examples: [
		{
			title: "Parse AWS CloudTrail log records"
			source: #"""
				parse_aws_cloudtrail_logs!(s'{
					"Records": [{
						"eventVersion": "1.0",
						"userIdentity": {
							"type": "IAMUser",
							"principalId": "EX_PRINCIPAL_ID",
							"arn": "arn:aws:iam::123456789012:user/Alice",
							"accessKeyId": "EXAMPLE_KEY_ID",
							"accountId": "123456789012",
							"userName": "Alice"
						},
						"eventTime": "2014-03-06T21:22:54Z",
						"eventSource": "ec2.amazonaws.com",
						"eventName": "StartInstances",
						"awsRegion": "us-east-2",
						"sourceIPAddress": "205.251.233.176",
						"userAgent": "ec2-api-tools 1.6.12.2",
						"requestParameters": {
							"instancesSet": {
								"items": [{
									"instanceId": "i-ebeaf9e2"
								}]
							}
						},
						"responseElements": {
							"instancesSet": {
								"items": [{
									"instanceId": "i-ebeaf9e2",
									"currentState": {
										"code": 0,
										"name": "pending"
									},
									"previousState": {
										"code": 80,
										"name": "stopped"
									}
								}]
							}
						}
					}]
				}')
				"""#
			return: [{
				aws_region:    "us-east-2"
				event_name:    "StartInstances"
				event_source:  "ec2.amazonaws.com"
				event_time:    "2014-03-06T21:22:54Z"
				event_version: "1.0"
				request_parameters: {
					instancesSet: {
						items: [
							{
								instanceId: "i-ebeaf9e2"
							},
						]
					}
				}
				response_elements: {
					instancesSet: {
						items: [
							{
								currentState: {
									code: 0
									name: "pending"
								}
								instanceId: "i-ebeaf9e2"
								previousState: {
									code: 80
									name: "stopped"
								}
							},
						]
					}
				}
				source_ip_address: "205.251.233.176"
				user_agent:        "ec2-api-tools 1.6.12.2"
				user_identity: {
					access_key_id: "EXAMPLE_KEY_ID"
					account_id:    "123456789012"
					arn:           "arn:aws:iam::123456789012:user/Alice"
					principal_id:  "EX_PRINCIPAL_ID"
					type:          "IamUser"
					user_name:     "Alice"
				}
			}]
		},
	]
}
