Fix disk-buffer shutdown waiting for partial sink batches to reach their timeout. Sinks such as
`aws_s3` now flush partial batches when buffered input ends and complete acknowledgement checkpointing
before the sink finishes shutting down. Fixes #25774.

authors: garryliu-stripe
