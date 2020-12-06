package metadata

services: aws_s3: {
	name:     "AWS S3"
	thing:    "an \(name) bucket"
	url:      urls.aws_s3
	versions: null

	description: "[Amazon Simple Storage Service (Amazon S3)](\(urls.aws_s3)) is a scalable, high-speed, web-based cloud storage service designed for online backup and archiving of data and applications on Amazon Web Services. It is very commonly used to store log data."

	connect_to: {
		vector: logs: {
			setup: [
				{
					title: "Create an AWS SQS queue"
					description: """
						Create an AWS SQS queue for Vector to consume bucket notifications from.
						"""
					detour: url: urls.aws_sqs_create
				},
				{
					title: "Publish S3 bucket notifications to the queue"
					description: """
						Configure S3 to publish Bucket notifications to your previously created SQS queue.
						Ensure that it only publishes the following events:

						- PUT
						- POST
						- COPY
						- Multipart upload completed

						These represent object creation events and ensure Vector does not double process
						S3 objects.
						"""
					detour: url: urls.aws_s3_bucket_notifications_to_sqs
				},
				{
					title: "Configure Vector"
					description: """
						Using the SQS queue URL provided to you by AWS, configure the Vector `aws_s3`
						source to use the SQS queue via the `sqs.queue_url` option.
						"""
					vector: configure: sources: aws_s3: {
						type: "aws_s3"
						sqs: queue_url: "<sqs-que-url>"
					}
				},
			]
		}
	}
}
