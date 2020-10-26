package metadata

components: sources: aws_s3: {
	title:       "AWS S3"
	description: "[Amazon Simple Storage Service (Amazon S3)][urls.aws_s3] is a scalable, high-speed, web-based cloud storage service designed for online backup and archiving of data and applications on Amazon Web Services. It is very commonly used to store log data."

	features: {
		multiline: enabled: true
		tls: enabled:       false
		collect: {
			from: {
				name:     "AWS S3"
				thing:    "a \(name) bucket"
				url:      urls.aws_s3
				versions: null

				setup: [
					"""
						Create an AWS SQS queue for Vector to consume bucket
						notifications from. Then, configure the [bucket
						notifications](https://docs.aws.amazon.com/AmazonS3/latest/dev/ways-to-add-notification-config-to-bucket.html)
						to publish to this queue for the following events:

						- PUT
						- POST
						- COPY
						- Multipart upload completed

						These represent object creation events.
						""",
				]
			}
		}
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
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

	configuration: components._aws.configuration & {
		strategy: {
			common:      true
			description: "The strategy to use to consume objects from AWS S3."
			required:    false
			type: string: {
				default: "sqs"
				enum: {
					sqs: "Consume S3 objects by polling for bucket notifications sent to an SQS queue."
				}
			}
		}
		compression: {
			common:      false
			description: "The compression format of the S3 objects.."
			required:    false
			type: string: {
				default: "text"
				enum: {
					auto: "Vector will try to determine the compression format of the object from its: `Content-Encoding` metadata, `Content-Type` metadata, and key suffix (e.g. `.gz`). It will fallback to 'none' if it cannot determine the compression."
					gzip: "GZIP format."
					zstd: "ZSTD format."
					none: "Uncompressed."
				}
			}
		}
		sqs: {
			common:      true
			description: "SQS strategy options. Required if strategy=`sqs`."
			required:    true
			type: object: {
				examples: []
				options: {
					poll_secs: {
						common:      true
						description: "How often to poll the queue for new messages in seconds."
						required:    false
						warnings: []
						type: int: default: 15
					}
					visibility_timeout_secs: {
						common:      false
						description: "The visibility timeout to use for messages in secords. This controls how long a message is left unavailable when a Vector receives it. If Vector does not delete the message before the timeout expires, it will be made reavailable for another consumer; this can happen if, for example, the `vector` process crashes."
						required:    false
						warnings: ["Should be set higher than the length of time it takes to process an individual message to avoid that message being reprocessed."]
						type: int: default: 300
					}
					delete_message: {
						common:      true
						description: "Whether to delete the message once Vector processes it. It can be useful to set this to `false` to debug or during initial Vector setup."
						required:    false
						warnings: []
						type: bool: default: true
					}
					queue_name: {
						common:      true
						description: "The name of the SQS queue to receieve bucket notifications from."
						required:    true
						warnings: []
						type: string: {
							default: null
							examples: ["my-queue-name"]
						}
					}
					queue_owner: {
						common:      false
						description: "The AWS account ID of the owner of the queue. This is only needed if AWS user or role that Vector is using is different than the queue owner."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["123456789012"]
						}
					}
				}
			}
		}
	}

	output: logs: object: {
		description: "A line from an S3 object."
		fields: {
			message: {
				description: "A line from the S3 object."
				required:    true
				type: string: examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
			}
			timestamp: fields._current_timestamp & {
				description: "The Last-Modified time of the object. Defaults the current timestamp if this information is missing."
			}
			bucket: {
				description: "The bucket of the object the line came from."
				required:    true
				type: string: examples: ["my-bucket"]
			}
			object: {
				description: "The object the line came from."
				required:    true
				type: string: examples: ["AWSLogs/111111111111/vpcflowlogs/us-east-1/2020/10/26/111111111111_vpcflowlogs_us-east-1_fl-0c5605d9f1baf680d_20201026T1950Z_b1ea4a7a.log.gz"]
			}
			region: {
				description: "The AWS region bucket is in."
				required:    true
				type: string: examples: ["us-east-1"]
			}
		}
	}

	//how_it_works: components._kafka.how_it_works
}
