package metadata

services: aws_sqs: {
	name:     "AWS Simple Queue Service"
	thing:    "an \(name) queue"
	url:      urls.aws_sqs
	versions: null

	description: "[Amazon Simple Queue Service (SQS)](\(urls.aws_sqs)) is a fully managed message queuing service that enables you to decouple and scale microservices, distributed systems, and serverless applications."
}
