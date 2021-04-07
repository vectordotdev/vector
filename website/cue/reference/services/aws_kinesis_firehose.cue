package metadata

services: aws_kinesis_firehose: {
	name:     "AWS Kinesis Firehose"
	thing:    "a \(name) stream"
	url:      urls.aws_kinesis_firehose
	versions: null

	description: """
		[Amazon Kinesis Data Firehose](\(urls.aws_kinesis_firehose)) is a fully
		managed service for delivering real-time streaming data to destinations
		such as Amazon Simple Storage Service (Amazon S3), Amazon Redshift,
		Amazon Elasticsearch Service (Amazon ES), and Splunk.
		"""

	connect_to: {
		vector: logs: {
			_address: "0.0.0.0:443"

			setup: [
				{
					title: "Configure Vector to accept AWS Kinesis Firehose data"
					vector: configure: sources: aws_kinesis_firehose: {
						type:       "aws_kinesis_firehose"
						address:    _address
						access_key: "A94A8FE5CCB19BA61C4C08"
						region:     "us-east-1"
					}
				},
				{
					title: "Configure TLS termination"
					description: """
						AWS Kinesis Firehose will only forward to HTTPS (and not HTTP)
						endpoints running on port 443. You will need to either put a load
						balancer in front of the Vector instance to handle TLS termination
						or configure the `tls` options of the Vector `aws_kinesis_firehose`
						source to serve a valid certificate.
						"""
					detour: url: urls.aws_elb_https
				},
				{
					title: "Create an AWS Kinesis Firehose HTTP Stream"
					description: """
						Using your previously configured TLS enabled HTTP endpoint,
						let's create a Kinesis Firehose HTTP stream that delivers
						data to it. Be sure to use your HTTP endpoint.
						"""
					detour: url: urls.aws_kinesis_firehose_http_setup
				},
			]
		}
	}
}
