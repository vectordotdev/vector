package metadata

services: aws_cloudwatch_logs: {
	name:     "AWS Cloudwatch logs"
	thing:    "an \(name) stream"
	url:      urls.aws_cloudwatch_logs
	versions: null

	description: services._aws_cloudwatch.description

	connect_to: {
		aws_kinesis_firehose: logs: {
			setup: [
				{
					title: "Stream CloudWatch logs to Firehose"
					description: """
						Using your configured AWS Firehose delivery stream, we'll need to
						stream AWS Cloudwatch Logs to it. We achieve this through AWS Cloudwatch Logs
						subscriptions.
						"""
					detour: url: urls.aws_cloudwatch_logs_subscriptions_firehose
				},
			]
		}
		aws_s3: logs: {
			description: """
				AWS Cloudwatch logs can export log data to S3 which can then be
				imported by Vector via the `aws_s3` source. Please note, this is
				a single export, not a stream of data. If you want Vector to
				continuously ingest AWS Cloudwatch logs data you will need to
				follow the AWS Cloudwatch logs to AWS Kinesis tutorial.
				"""
			setup: [
				{
					title: "Export AWS Cloudwatch logs data to AWS S3"
					description: """
						Follow the AWS CloudWatch to S3 export guide to export
						your Cloudwatch logs data to the S3 bucket of your choice.
						"""
					detour: url: urls.aws_cloudwatch_logs_s3_export
				},
			]
		}
	}
}
