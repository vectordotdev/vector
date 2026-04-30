Constrained the `aws_s3` source's `sqs.poll_secs` and the `aws_sqs` source's `poll_secs` configuration fields to the AWS-imposed maximum of 20 seconds. These fields map to SQS `ReceiveMessage`'s `WaitTimeSeconds` parameter; previously, values above 20 caused AWS to reject the call, manifesting as silent ingestion failure with no error or hint in the documentation. The field documentation now states the limit explicitly, and `vector validate` rejects out-of-range configurations.

authors: st-omarkhalid
