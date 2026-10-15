The `aws_s3` source now accepts CrowdStrike Falcon Data Replicator (FDR) SQS notifications and processes their referenced S3 objects using the existing compression and acknowledgement settings. Age-based deferral uses FDR batch publication timestamps and shares the existing S3 forwarding and deletion behavior.

authors: jacqueline-chin_ddog
