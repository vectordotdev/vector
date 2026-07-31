package metadata

services: aws_sns: {
	name:     "AWS Simple Notification Service"
	thing:    "an \(name) topic"
	url:      urls.aws_sns
	versions: null

	description: "[Amazon Simple Notification Service (SNS)](\(urls.aws_sns)) is a fully managed pub/sub messaging service that enables you to decouple microservices, distributed systems, and serverless applications."
}
