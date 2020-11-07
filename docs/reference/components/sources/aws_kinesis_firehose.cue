package metadata

components: sources: aws_kinesis_firehose: {
	_port: 443

	title:       "AWS Kinesis Firehose"
	description: "[AWS Kinesis Firehose](\(urls.aws_kinesis_firehose)) is an AWS service that simplifies dealing with streaming data. It allows for ingestion, transformation, and forwarding of events. In addition to publishing events directly to Kinesis Firehose, the service has direct integrations with many AWS services which allow them to directly publish events to a delivery stream."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				name:     "AWS Kinesis Firehose"
				thing:    "a \(name) stream"
				url:      urls.aws_kinesis_firehose
				versions: null

				interface: socket: {
					api: {
						title: "AWS Kinesis Firehose HTTP Destination"
						url:   urls.aws_firehose_http_request_spec
					}
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "required"
				}

				setup: [
					"""
						[Setup a Kinesis Firehose delivery stream](\(urls.aws_kinesis_firehose_setup))
						in your preferred AWS region. Point the endpoint to your
						Vector instance's address.
						""",
				]
			}

			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}}
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
		address: {
			description: "The address to listen for connections on"
			required:    true
			type: string: examples: ["0.0.0.0:443", "localhost:443"]
		}
		access_key: {
			common: true
			description: """
					AWS Kinesis Firehose can be configured to pass along an access
					key to authenticate requests. If configured, `access_key` should
					be set to the same value. If not specified, vector will treat
					all requests as authenticated.
				"""
			required: false
			type: "string": {
				default: null
				examples: ["A94A8FE5CCB19BA61C4C08"]
			}
		}
	}

	output: logs: {
		line: {
			description: "One event will be published per incoming AWS Kinesis Firehose record."
			fields: {
				timestamp: fields._current_timestamp
				message: {
					description: "The raw record from the incoming payload."
					required:    true
					type: string: examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
				}
				request_id: {
					description: "The AWS Kinesis Firehose request ID, value of the `X-Amz-Firehose-Request-Id` header."
					required:    true
					type: string: examples: ["ed1d787c-b9e2-4631-92dc-8e7c9d26d804"]
				}
				source_arn: {
					description: "The AWS Kinises Firehose delivery stream that issued the request, value of the `X-Amz-Firehose-Source-Arn` header."
					required:    true
					type: string: examples: ["arn:aws:firehose:us-east-1:111111111111:deliverystream/test"]
				}
			}
		}
	}

	examples: [
		{
			title: "AWS CloudWatch Subscription message"
			configuration: {
				address: "0.0.0.0:443"
			}
			input: """
				```json
					{
						"requestId": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
						"timestamp": 1600110760138,
						"records": [
							{
								"data": "H4sIABk1bV8AA52TzW7bMBCE734KQ2db/JdI3QzETS8FAtg91UGgyOuEqCQq5Mqua+TdS8lu0hYNUpQHAdoZDcn9tKfJdJo0EEL5AOtjB0kxTa4W68Xdp+VqtbheJrPB4A4t+EFiv6yzVLuHa+/6blARAr5UV+ihbH4vh/4+VN52aF37wdYIPkTDlyhF8SrabFsOWhIrtz+Dlnto8dV3Gp9RstshXKhMi0xpqk3GpNJccpFRKYw0WvCM5kIbzrVWipm4VK55rrSk44HGHLTx/lg2wxVYRiljVGWGCvPiuPRn2O60Se6P8UKbpOBZrulsk2xLhCEjljYJk2QFHeGU04KxQqpCsumcSko3SfQ+uoBnn8pTJmjKWZYyI0axAXx021G++bweS5136CpXj8WP6/UNYek5ycMOPPhReETsQkHI4XBIO2/bynZlXXkXwryrS9w536TWkab0XwED6e/tU2/R9eGS9NTD5VgEvnWwtQikcu0e/AO0FYyu4HpfwR3Gf2R0Btza9qxgiUNUISiLr30AP7fbyMzu7OWA803ynIzdfJ69B1EZpoVhsWMRZ8a5UVJoRoUyUlDNspxzZWiEnOXiXYiSvQOR5TnN/xsiNalmKZcy5Yr/yfB6+RZD/gbDC0IbOx8wQrMhxGGYx4lBW5X1wJBLkpO981jWf6EXogvIrm+rYYrKOn4Hgbg4b439/s8cFeVvcNwBtHBkOdWvQIdRnTxPfgCXvyEgSQQAAA=="
							}
						]
					}
				```
				"""
			output: [{
				log: {
					request_id: "ed1d787c-b9e2-4631-92dc-8e7c9d26d804"
					source_arn: "arn:aws:firehose:us-east-1:111111111111:deliverystream/test"
					timestamp:  "2020-09-14T19:12:40.138Z"
					message:    "{\"messageType\":\"DATA_MESSAGE\",\"owner\":\"111111111111\",\"logGroup\":\"test\",\"logStream\":\"test\",\"subscriptionFilters\":[\"Destination\"],\"logEvents\":[{\"id\":\"35683658089614582423604394983260738922885519999578275840\",\"timestamp\":1600110569039,\"message\":\"{\\\"bytes\\\":26780,\\\"datetime\\\":\\\"14/Sep/2020:11:45:41 -0400\\\",\\\"host\\\":\\\"157.130.216.193\\\",\\\"method\\\":\\\"PUT\\\",\\\"protocol\\\":\\\"HTTP/1.0\\\",\\\"referer\\\":\\\"https://www.principalcross-platform.io/markets/ubiquitous\\\",\\\"request\\\":\\\"/expedite/convergence\\\",\\\"source_type\\\":\\\"stdin\\\",\\\"status\\\":301,\\\"user-identifier\\\":\\\"-\\\"}\"},{\"id\":\"35683658089659183914001456229543810359430816722590236673\",\"timestamp\":1600110569041,\"message\":\"{\\\"bytes\\\":17707,\\\"datetime\\\":\\\"14/Sep/2020:11:45:41 -0400\\\",\\\"host\\\":\\\"109.81.244.252\\\",\\\"method\\\":\\\"GET\\\",\\\"protocol\\\":\\\"HTTP/2.0\\\",\\\"referer\\\":\\\"http://www.investormission-critical.io/24/7/vortals\\\",\\\"request\\\":\\\"/scale/functionalities/optimize\\\",\\\"source_type\\\":\\\"stdin\\\",\\\"status\\\":502,\\\"user-identifier\\\":\\\"feeney1708\\\"}\"}]}"
				}
			}]
		},
	]

	how_it_works: {
		structured_events: {
			title: "Forwarding CloudWatch Log events"
			body: """
				This source is the recommended way to ingest logs from AWS
				CloudWatch logs via [AWS CloudWatch Log
				subscriptions][aws_cloudwatch_logs_subscriptions]. To
				set this up:

				1. Deploy vector with a publicly exposed HTTP endpoint using
				   this source. You will likely also want to use the
				   [`aws_cloudwatch_logs_subscription_parser`][vector_transform_aws_cloudwatch_logs_subscription_parser]
				   transform to extract the log events.. Make sure to set
				   the `access_key` to secure this endpoint. Your
				   configuration might look something like:

				   ```toml
						[sources.firehose]
						  # General
						  type = "aws_kinesis_firehose"
						  address = "127.0.0.1:9000"
						  access_key = "secret"

						[transforms.cloudwatch]
						  type = "aws_cloudwatch_logs_subscription_parser"
						  inputs = ["firehose"]

						[sinks.console]
						  type = "console"
						  inputs = ["cloudwatch"]
						  encoding.codec = "json"
				   ```

				2. Create a Kinesis Firewatch delivery stream in the region
				   where the CloudWatch Logs groups exist that you want to
				   ingest.
				3. Set the stream to forward to your Vector instance via its
				   HTTP Endpoint destination. Make sure to configure the
				   same `access_key` you set earlier.
				4. Setup a [CloudWatch Logs
				   subscription][aws_cloudwatch_logs_subscriptions] to
				   forward the events to your delivery stream
				"""
		}
	}

	telemetry: metrics: {
		vector_request_read_errors_total: _vector_request_read_errors_total
		vector_requests_received_total:   _vector_requests_received_total
	}
}
