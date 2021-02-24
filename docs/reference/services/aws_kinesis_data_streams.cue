package metadata

services: aws_kinesis_data_streams: {
	name:     "AWS Kinesis Data Streams"
	thing:    "a \(name) stream"
	url:      urls.aws_kinesis_streams
	versions: null

	description: "[Amazon Kinesis Data Streams](\(urls.aws_kinesis_streams)) is a scalable and durable real-time data streaming service that can continuously capture gigabytes of data per second from hundreds of thousands of sources. Making it an excellent candidate for streaming logs and metrics data."
}
