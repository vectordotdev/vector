package metadata

services: aws_cloudwatch: {
	name:     "AWS Cloudwatch logs"
	thing:    "an \(name) stream"
	url:      urls.aws_cloudwatch_logs
	versions: null
}
