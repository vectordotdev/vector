Fixed the blackhole sink to properly implement end-to-end acknowledgements. Previously, the sink consumed events without updating finalizer status, causing sources that depend on acknowledgements (like `aws_s3` with SQS) to never delete processed messages from the queue.

authors: sanjams2
