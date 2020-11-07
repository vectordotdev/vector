package metadata

components: transforms: aws_cloudwatch_logs_subscription_parser: {
	title:       "AWS CloudWatch Logs Subscription Parser"
	description: "[AWS CloudWatch Logs Subscription events](\(urls.aws_cloudwatch_logs_subscriptions)) allow you to forward [AWS CloudWatch Logs](\(urls.aws_cloudwatch_logs)) to external systems. Through the subscriiption, you can: call a Lambda, send to AWS Kinesis, or send to AWS Kinesis Firehose (which can then be forwarded to many destinations)."

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		parse: {
			format: {
				name:     "AWS CloudWatch Logs subscription events"
				url:      urls.aws_cloudwatch_logs_subscriptions
				versions: null
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		field: {
			common:      true
			description: "The log field to decode as an AWS CloudWatch Logs Subscription JSON event. The field must hold a string value."
			required:    false
			warnings: []
			type: string: default: "message"
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	output: logs: line: {
		description: "One event will be published per log event in the subscription message."
		fields: {
			timestamp: {
				description: "The timestamp of the log event."
				required:    true
				type: timestamp: {}
			}
			message: {
				description: "The body of the log event."
				required:    true
				type: string: examples: ["hello", "{\"key\": \"value\"}"]
			}
			id: {
				description: "The CloudWatch Logs event id."
				required:    true
				type: string: examples: ["35683658089614582423604394983260738922885519999578275840"]
			}
			log_group: {
				description: "The log group the event came from."
				required:    true
				type: string: examples: ["/lambda/test"]
			}
			log_stream: {
				description: "The log stream the event came from."
				required:    true
				type: string: examples: ["2020/03/24/[$LATEST]794dbaf40a7846c4984ad80ebf110544"]
			}
			owner: {
				description: "The ID of the AWS account the logs came from."
				required:    true
				type: string: examples: ["111111111111"]
			}
			subscription_filters: {
				description: "The list of subscription filter names that the logs were sent by."
				required:    true
				type: array: items: type: string: examples: ["Destination"]
			}
		}
	}

	examples: [
		{
			title: "Default"
			configuration: {
				field: "message"
			}
			input: log: {
				message: """
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
							},
							{
							  "id": "35683658089659183914001456229543810359430816722590236673",
							  "timestamp": 1600110569041,
							  "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
							}
						  ]
						}
					"""
			}
			output: {
				log: {
					id:         "35683658089614582423604394983260738922885519999578275840"
					log_group:  "test"
					log_stream: "test"
					message:    "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-latform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
					owner:      "111111111111"
					timestamp:  "2020-09-14T19:09:29.039Z"
					subscription_filters: [ "Destination"]
				}
			}
		},
	]

	how_it_works: {
		structured_events: {
			title: "Structured Log Events"
			body:  "Note that the events themselves are not parsed. If they are structured data, you will typically want to pass them through a [parsing transform](\(urls.vector_parsing_transforms))."
		}
	}

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
