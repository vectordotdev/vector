package metadata

services: aws_cloudwatch_metrics: {
	name:     "AWS Cloudwatch metrics"
	thing:    "an \(name) namespace"
	url:      urls.aws_cloudwatch_metrics
	versions: null

	description: services._aws_cloudwatch.description
}
